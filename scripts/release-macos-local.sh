#!/usr/bin/env bash

set -Eeuo pipefail
umask 022

fail() {
  printf 'macOS release error: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command '$1' was not found."
}

github_repository_from_url() {
  local repository_url="${1#git+}"
  repository_url="${repository_url%.git}"
  if [[ "$repository_url" =~ github\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi
  return 1
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  fail "this release command must run locally on macOS."
fi

for command_name in \
  npm cargo rustup security xcrun codesign spctl hdiutil gzip grep jq \
  git gh awk shasum stat date head; do
  require_command "$command_name"
done

script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repository_root="$(cd -- "$script_directory/.." && pwd -P)"
cd "$repository_root"

version="$(jq -er '.version | select(type == "string" and length > 0)' package.json)" \
  || fail "package.json does not contain a valid version."
tauri_version="$(jq -er '.version | select(type == "string" and length > 0)' src-tauri/tauri.conf.json)" \
  || fail "src-tauri/tauri.conf.json does not contain a valid version."
cargo_version="$(cargo metadata \
  --manifest-path src-tauri/Cargo.toml \
  --format-version 1 \
  --no-deps \
  | jq -er '.packages[] | select(.name == "onyx-desktop") | .version')" \
  || fail "could not resolve the native Cargo package version."
[[ "$version" == "$tauri_version" && "$version" == "$cargo_version" ]] \
  || fail "package.json ($version), Tauri ($tauri_version), and Cargo ($cargo_version) versions must match."
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]] \
  || fail "version '$version' is not a supported semantic version."
release_tag="v$version"

source_repository_url="$(jq -er '
  if (.repository | type) == "object" then .repository.url
  elif (.repository | type) == "string" then .repository
  else empty
  end
' package.json)" || fail "package.json does not define a source repository."
source_repository="$(github_repository_from_url "$source_repository_url")" \
  || fail "package.json repository must be a GitHub repository."
origin_url="$(git remote get-url origin 2>/dev/null)" \
  || fail "the source repository has no origin remote."
origin_repository="$(github_repository_from_url "$origin_url")" \
  || fail "origin '$origin_url' is not a supported GitHub URL."
[[ "$origin_repository" == "$source_repository" ]] \
  || fail "origin '$origin_url' does not match source repository '$source_repository'."

release_repository="${ONYX_RELEASE_REPOSITORY:-}"
configured_updater_endpoint="$(jq -er '
  .plugins.updater.endpoints
  | select(type == "array" and length > 0)
  | .[0]
  | select(type == "string" and length > 0)
' src-tauri/tauri.conf.json)" || fail "Tauri does not define an updater endpoint."
if [[ -z "$release_repository" ]]; then
  if [[ "$configured_updater_endpoint" =~ ^https://github\.com/([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)/releases/latest/download/latest\.json$ ]]; then
    release_repository="${BASH_REMATCH[1]}"
  else
    fail "the updater endpoint is not a supported GitHub Releases URL."
  fi
fi
[[ "$release_repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] \
  || fail "ONYX_RELEASE_REPOSITORY must use the OWNER/REPO form."
expected_updater_endpoint="https://github.com/$release_repository/releases/latest/download/latest.json"
[[ "$configured_updater_endpoint" == "$expected_updater_endpoint" ]] \
  || fail "the embedded updater endpoint must be '$expected_updater_endpoint' for this release."

release_repository_visibility="$(gh repo view "$release_repository" --json visibility --jq '.visibility')" \
  || fail "release repository '$release_repository' does not exist or is not accessible."
[[ "$release_repository_visibility" == "PUBLIC" ]] \
  || fail "release repository '$release_repository' must be public so installed apps can update."

if [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  fail "the Git worktree must be clean before a production release build."
fi
local_commit="$(git rev-parse HEAD)" \
  || fail "could not resolve local HEAD."
local_tag_commit="$(git rev-parse "$release_tag^{commit}" 2>/dev/null)" \
  || fail "local tag '$release_tag' does not exist."
[[ "$local_tag_commit" == "$local_commit" ]] \
  || fail "local tag '$release_tag' does not point to HEAD."
remote_tag_lines="$(git ls-remote --tags origin \
  "refs/tags/$release_tag" "refs/tags/$release_tag^{}")" \
  || fail "could not resolve '$release_tag' from origin."
remote_tag_commit="$(awk '
  $2 ~ /\^\{\}$/ { peeled = $1 }
  $2 !~ /\^\{\}$/ { direct = $1 }
  END { if (peeled != "") print peeled; else print direct }
' <<<"$remote_tag_lines")"
[[ -n "$remote_tag_commit" && "$remote_tag_commit" == "$local_commit" ]] \
  || fail "origin tag '$release_tag' does not resolve to local HEAD."

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -z "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
  fail "set TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH for updater signing."
fi

if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" && -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
  fail "set only one of TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH."
fi

if [[ -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]] \
  && [[ ! -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" || ! -r "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]]; then
  fail "TAURI_SIGNING_PRIVATE_KEY_PATH does not point to a readable regular file."
fi

if [[ "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD+x}" != "x" ]]; then
  fail "declare TAURI_SIGNING_PRIVATE_KEY_PASSWORD (export it as an empty string for an unencrypted key)."
fi

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" || "$APPLE_SIGNING_IDENTITY" == "-" ]]; then
  fail "set APPLE_SIGNING_IDENTITY to a Developer ID Application identity."
fi

if [[ "$APPLE_SIGNING_IDENTITY" != "Developer ID Application:"* ]]; then
  fail "APPLE_SIGNING_IDENTITY must be a Developer ID Application identity for direct distribution."
fi

available_identities="$(security find-identity -v -p codesigning 2>/dev/null)" \
  || fail "could not inspect signing identities in the macOS Keychain."
if ! grep -Fq -- "\"$APPLE_SIGNING_IDENTITY\"" <<<"$available_identities"; then
  unset available_identities
  fail "APPLE_SIGNING_IDENTITY was not found as a valid code-signing identity in the Keychain."
fi
unset available_identities

api_credentials_requested=false
apple_id_credentials_requested=false
if [[ -n "${APPLE_API_ISSUER:-}" || -n "${APPLE_API_KEY:-}" || -n "${APPLE_API_KEY_PATH:-}" ]]; then
  api_credentials_requested=true
fi
if [[ -n "${APPLE_ID:-}" || -n "${APPLE_PASSWORD:-}" || -n "${APPLE_TEAM_ID:-}" ]]; then
  apple_id_credentials_requested=true
fi

if [[ "$api_credentials_requested" == true ]]; then
  [[ -n "${APPLE_API_ISSUER:-}" ]] \
    || fail "APPLE_API_ISSUER is required with App Store Connect API notarization."
  [[ -n "${APPLE_API_KEY:-}" ]] \
    || fail "APPLE_API_KEY is required with App Store Connect API notarization."
  [[ -n "${APPLE_API_KEY_PATH:-}" ]] \
    || fail "APPLE_API_KEY_PATH is required with App Store Connect API notarization."
  [[ -r "$APPLE_API_KEY_PATH" ]] \
    || fail "APPLE_API_KEY_PATH does not point to a readable file."
elif [[ "$apple_id_credentials_requested" == true ]]; then
  [[ -n "${APPLE_ID:-}" ]] \
    || fail "APPLE_ID is required for Apple ID notarization."
  [[ -n "${APPLE_PASSWORD:-}" ]] \
    || fail "APPLE_PASSWORD must contain an app-specific password for notarization."
  [[ -n "${APPLE_TEAM_ID:-}" ]] \
    || fail "APPLE_TEAM_ID is required for Apple ID notarization."
else
  fail "set either APPLE_API_ISSUER/APPLE_API_KEY/APPLE_API_KEY_PATH or APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID for notarization."
fi

notarization_timeout="${ONYX_NOTARIZATION_TIMEOUT:-30m}"
[[ "$notarization_timeout" =~ ^[1-9][0-9]*(s|m|h)?$ ]] \
  || fail "ONYX_NOTARIZATION_TIMEOUT must be a positive duration such as 1800s, 30m, or 1h."

macos_target="${ONYX_MACOS_TARGET:-universal-apple-darwin}"
case "$macos_target" in
  universal-apple-darwin)
    required_rust_targets=(aarch64-apple-darwin x86_64-apple-darwin)
    ;;
  aarch64-apple-darwin | x86_64-apple-darwin)
    required_rust_targets=("$macos_target")
    ;;
  *)
    fail "ONYX_MACOS_TARGET must be universal-apple-darwin, aarch64-apple-darwin, or x86_64-apple-darwin."
    ;;
esac

installed_rust_targets="$(rustup target list --installed)"
for rust_target in "${required_rust_targets[@]}"; do
  if ! grep -Fxq -- "$rust_target" <<<"$installed_rust_targets"; then
    fail "Rust target '$rust_target' is missing. Install it with: rustup target add $rust_target"
  fi
done
unset installed_rust_targets

printf 'Building signed and notarized Onyx bundles locally for %s...\n' "$macos_target"
npm run build:desktop -- --target "$macos_target"

[[ "$(git rev-parse HEAD)" == "$local_commit" ]] \
  || fail "HEAD changed while the release artifacts were being built."
if [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  fail "the build changed the source worktree; release provenance cannot be trusted."
fi

bundle_root="$repository_root/src-tauri/target/$macos_target/release/bundle"
[[ -d "$bundle_root" ]] || fail "Tauri completed without creating the expected bundle directory."

shopt -s nullglob
app_bundles=("$bundle_root"/macos/*.app)
dmg_bundles=("$bundle_root"/dmg/*.dmg)
updater_archives=("$bundle_root"/macos/*.app.tar.gz)
updater_signatures=("$bundle_root"/macos/*.app.tar.gz.sig)
shopt -u nullglob

[[ "${#app_bundles[@]}" -eq 1 ]] \
  || fail "expected exactly one macOS .app bundle, found ${#app_bundles[@]}."
[[ "${#dmg_bundles[@]}" -eq 1 ]] \
  || fail "expected exactly one macOS DMG, found ${#dmg_bundles[@]}."
[[ "${#updater_archives[@]}" -eq 1 ]] \
  || fail "expected exactly one macOS updater archive, found ${#updater_archives[@]}."
[[ "${#updater_signatures[@]}" -eq 1 ]] \
  || fail "expected exactly one updater signature, found ${#updater_signatures[@]}."

app_bundle="${app_bundles[0]}"
dmg_bundle="${dmg_bundles[0]}"
updater_archive="${updater_archives[0]}"
updater_signature="${updater_signatures[0]}"

codesign --verify --deep --strict "$app_bundle" \
  || fail "codesign verification failed for the generated app."
xcrun stapler validate "$app_bundle" \
  || fail "the generated app has no valid stapled notarization ticket."
spctl --assess --type execute "$app_bundle" \
  || fail "Gatekeeper rejected the generated app; signing or notarization is incomplete."
hdiutil verify "$dmg_bundle" >/dev/null \
  || fail "the generated DMG failed verification."

notarytool_arguments=(
  notarytool submit "$dmg_bundle"
  --wait
  --timeout "$notarization_timeout"
  --output-format json
)
if [[ "$api_credentials_requested" == true ]]; then
  notarytool_arguments+=(
    --key "$APPLE_API_KEY_PATH"
    --key-id "$APPLE_API_KEY"
    --issuer "$APPLE_API_ISSUER"
  )
else
  notarytool_arguments+=(
    --apple-id "$APPLE_ID"
    --password "$APPLE_PASSWORD"
    --team-id "$APPLE_TEAM_ID"
  )
fi
dmg_notarization_result="$(
  xcrun "${notarytool_arguments[@]}" | head -c 65537
)" \
  || fail "Apple notary service submission failed for the generated DMG."
[[ "${#dmg_notarization_result}" -le 65536 ]] \
  || fail "Apple notary service returned an unexpectedly large response."
jq -e '.status == "Accepted"' <<<"$dmg_notarization_result" >/dev/null \
  || fail "Apple did not accept the generated DMG."
unset dmg_notarization_result notarytool_arguments

xcrun stapler staple "$dmg_bundle" \
  || fail "the accepted notarization ticket could not be stapled to the DMG."
xcrun stapler validate "$dmg_bundle" \
  || fail "the generated DMG has no valid stapled notarization ticket."
spctl --assess --type open --context context:primary-signature "$dmg_bundle" \
  || fail "Gatekeeper rejected the generated DMG."
gzip -t "$updater_archive" \
  || fail "the generated updater archive failed verification."
[[ -s "$updater_signature" ]] || fail "the updater signature is empty."

cargo run --quiet \
  --manifest-path src-tauri/Cargo.toml \
  --example verify_updater_signature \
  -- src-tauri/tauri.conf.json "$updater_archive" "$updater_signature" \
  || fail "the updater archive did not verify against the public key embedded in Onyx."

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

provenance_path="$bundle_root/onyx-macos-release-provenance.json"
commit_date="$(git show -s --format=%cI "$local_commit")"
built_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
jq -n \
  --arg version "$version" \
  --arg tag "$release_tag" \
  --arg commit "$local_commit" \
  --arg commit_date "$commit_date" \
  --arg built_at "$built_at" \
  --arg source_repository "$source_repository" \
  --arg release_repository "$release_repository" \
  --arg updater_endpoint "$configured_updater_endpoint" \
  --arg target "$macos_target" \
  --arg app_file "$(basename -- "$app_bundle")" \
  --arg dmg_file "$(basename -- "$dmg_bundle")" \
  --arg dmg_sha256 "$(sha256_file "$dmg_bundle")" \
  --argjson dmg_bytes "$(stat -f '%z' "$dmg_bundle")" \
  --arg archive_file "$(basename -- "$updater_archive")" \
  --arg archive_sha256 "$(sha256_file "$updater_archive")" \
  --argjson archive_bytes "$(stat -f '%z' "$updater_archive")" \
  --arg signature_file "$(basename -- "$updater_signature")" \
  --arg signature_sha256 "$(sha256_file "$updater_signature")" \
  --argjson signature_bytes "$(stat -f '%z' "$updater_signature")" \
  '{
    schemaVersion: 1,
    platform: "macos",
    target: $target,
    version: $version,
    source: {
      repository: $source_repository,
      tag: $tag,
      commit: $commit,
      commitDate: $commit_date,
      clean: true
    },
    release: {
      repository: $release_repository,
      updaterEndpoint: $updater_endpoint
    },
    builtAt: $built_at,
    artifacts: {
      app: {file: $app_file},
      dmg: {file: $dmg_file, sha256: $dmg_sha256, bytes: $dmg_bytes},
      updaterArchive: {
        file: $archive_file,
        sha256: $archive_sha256,
        bytes: $archive_bytes
      },
      updaterSignature: {
        file: $signature_file,
        sha256: $signature_sha256,
        bytes: $signature_bytes
      }
    }
  }' >"$provenance_path" \
  || fail "could not write the release provenance file."

printf 'Local macOS release artifacts are ready in:\n%s\n' "$bundle_root"
printf 'Checksums and source provenance: %s\n' "$provenance_path"
printf 'No files were uploaded or published.\n'
printf 'After the Windows workflow creates the draft release for %s, run:\n' \
  "$release_tag"
printf 'npm run release:macos:publish -- --target %s\n' "$macos_target"
