#!/usr/bin/env bash

set -Eeuo pipefail
umask 077

fail() {
  printf 'macOS publish error: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command '$1' was not found."
}

usage() {
  cat <<'EOF'
Publish an already-built, signed, and notarized Onyx macOS release.

Usage:
  npm run release:macos:publish -- [options]

Options:
  --target TARGET       universal-apple-darwin, aarch64-apple-darwin,
                        or x86_64-apple-darwin (default: ONYX_MACOS_TARGET
                        or universal-apple-darwin)
  --bundle-root PATH    Explicit Tauri bundle directory for TARGET
  --repo OWNER/REPO     Public release repository (default: updater endpoint)
  --tag vVERSION        Existing draft release tag; must match package.json
  --dry-run             Validate and render the merged manifest without uploads
  -h, --help            Show this help

This command never creates a tag or GitHub release. It validates the Windows
draft, uploads the notarized macOS artifacts, and publishes the release last.
EOF
}

dry_run=false
macos_target="${ONYX_MACOS_TARGET:-universal-apple-darwin}"
bundle_root_input="${ONYX_MACOS_BUNDLE_ROOT:-}"
repository_input="${ONYX_RELEASE_REPOSITORY:-}"
tag_input=""

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --target)
      [[ "$#" -ge 2 ]] || fail "--target requires a value."
      macos_target="$2"
      shift 2
      ;;
    --bundle-root)
      [[ "$#" -ge 2 ]] || fail "--bundle-root requires a value."
      bundle_root_input="$2"
      shift 2
      ;;
    --repo)
      [[ "$#" -ge 2 ]] || fail "--repo requires a value."
      repository_input="$2"
      shift 2
      ;;
    --tag)
      [[ "$#" -ge 2 ]] || fail "--tag requires a value."
      tag_input="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument '$1'. Run with --help for usage."
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  fail "this publish command must run locally on macOS."
fi

for command_name in \
  git gh jq awk gzip tar hdiutil plutil mktemp cp cmp cargo \
  basename dirname mkdir rm tr wc codesign spctl xcrun shasum stat; do
  require_command "$command_name"
done

github_repository_from_url() {
  local repository_url="${1#git+}"
  repository_url="${repository_url%.git}"
  if [[ "$repository_url" =~ github\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi
  return 1
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

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

tag="v$version"
if [[ -n "$tag_input" && "$tag_input" != "$tag" ]]; then
  fail "tag '$tag_input' does not match the application version '$tag'."
fi

case "$macos_target" in
  universal-apple-darwin)
    target_slug="universal"
    platform_keys=("darwin-aarch64" "darwin-x86_64")
    ;;
  aarch64-apple-darwin)
    target_slug="aarch64"
    platform_keys=("darwin-aarch64")
    ;;
  x86_64-apple-darwin)
    target_slug="x86_64"
    platform_keys=("darwin-x86_64")
    ;;
  *)
    fail "unsupported target '$macos_target'."
    ;;
esac

source_repository_url="$(jq -er '
  if (.repository | type) == "object" then .repository.url
  elif (.repository | type) == "string" then .repository
  else empty
  end
' package.json)" || fail "package.json does not define a source repository."
source_repository="$(github_repository_from_url "$source_repository_url")" \
  || fail "package.json repository must be a GitHub repository."

configured_updater_endpoint="$(jq -er '
  .plugins.updater.endpoints
  | select(type == "array" and length > 0)
  | .[0]
  | select(type == "string" and length > 0)
' src-tauri/tauri.conf.json)" || fail "Tauri does not define an updater endpoint."
if [[ -z "$repository_input" ]]; then
  if [[ "$configured_updater_endpoint" =~ ^https://github\.com/([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)/releases/latest/download/latest\.json$ ]]; then
    repository_input="${BASH_REMATCH[1]}"
  else
    fail "the updater endpoint is not a supported GitHub Releases URL."
  fi
fi
[[ "$repository_input" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] \
  || fail "repository must use the OWNER/REPO form."
repository="$repository_input"
expected_updater_endpoint="https://github.com/$repository/releases/latest/download/latest.json"
[[ "$configured_updater_endpoint" == "$expected_updater_endpoint" ]] \
  || fail "the embedded updater endpoint must be '$expected_updater_endpoint' for this release."

origin_url="$(git remote get-url origin 2>/dev/null)" \
  || fail "the repository has no origin remote."
origin_repository="$(github_repository_from_url "$origin_url")" \
  || fail "origin '$origin_url' is not a supported GitHub URL."
[[ "$origin_repository" == "$source_repository" ]] \
  || fail "origin '$origin_url' does not match source repository '$source_repository'."

if [[ -n "$(git status --porcelain=v1 --untracked-files=all)" ]]; then
  fail "the Git worktree must be clean before publishing release artifacts."
fi

local_commit="$(git rev-parse HEAD)" \
  || fail "could not resolve local HEAD."
local_tag_commit="$(git rev-parse "$tag^{commit}" 2>/dev/null)" \
  || fail "local tag '$tag' does not exist."
[[ "$local_tag_commit" == "$local_commit" ]] \
  || fail "local tag '$tag' does not point to HEAD."
remote_tag_lines="$(git ls-remote --tags origin "refs/tags/$tag" "refs/tags/$tag^{}")" \
  || fail "could not resolve '$tag' from source origin."
remote_tag_commit="$(awk '
  $2 ~ /\^\{\}$/ { peeled = $1 }
  $2 !~ /\^\{\}$/ { direct = $1 }
  END { if (peeled != "") print peeled; else print direct }
' <<<"$remote_tag_lines")"
[[ -n "$remote_tag_commit" && "$remote_tag_commit" == "$local_commit" ]] \
  || fail "source tag '$tag' does not resolve to local HEAD."

release_repository_json="$(gh api "repos/$repository")" \
  || fail "release repository '$repository' does not exist or is not accessible."
[[ "$(jq -r '.visibility' <<<"$release_repository_json")" == "public" ]] \
  || fail "release repository '$repository' must be public so installed apps can update."
[[ "$(jq -r '.permissions.push // false' <<<"$release_repository_json")" == "true" ]] \
  || fail "the current GitHub identity does not have push access to '$repository'."

if [[ -z "$bundle_root_input" ]]; then
  bundle_root_input="$repository_root/src-tauri/target/$macos_target/release/bundle"
elif [[ "$bundle_root_input" != /* ]]; then
  bundle_root_input="$repository_root/$bundle_root_input"
fi
[[ -d "$bundle_root_input" ]] \
  || fail "bundle root '$bundle_root_input' does not exist."
bundle_root="$(cd -- "$bundle_root_input" && pwd -P)"
case "$bundle_root/" in
  "$repository_root/src-tauri/target/"*) ;;
  *) fail "bundle root must be inside src-tauri/target." ;;
esac

shopt -s nullglob
app_candidates=("$bundle_root"/macos/*.app)
dmg_candidates=("$bundle_root"/dmg/*.dmg)
archive_candidates=("$bundle_root"/macos/*.app.tar.gz)
signature_candidates=("$bundle_root"/macos/*.app.tar.gz.sig)
shopt -u nullglob

[[ "${#app_candidates[@]}" -eq 1 ]] \
  || fail "expected exactly one app in '$bundle_root/macos', found ${#app_candidates[@]}."
[[ "${#dmg_candidates[@]}" -eq 1 ]] \
  || fail "expected exactly one DMG in '$bundle_root/dmg', found ${#dmg_candidates[@]}."
[[ "${#archive_candidates[@]}" -eq 1 ]] \
  || fail "expected exactly one updater archive in '$bundle_root/macos', found ${#archive_candidates[@]}."
[[ "${#signature_candidates[@]}" -eq 1 ]] \
  || fail "expected exactly one updater signature in '$bundle_root/macos', found ${#signature_candidates[@]}."

app_path="${app_candidates[0]}"
dmg_path="${dmg_candidates[0]}"
archive_path="${archive_candidates[0]}"
signature_path="${signature_candidates[0]}"
provenance_path="$bundle_root/onyx-macos-release-provenance.json"
[[ -d "$app_path" && ! -L "$app_path" ]] \
  || fail "the app must be a regular application bundle, not a symlink."
[[ -f "$dmg_path" && ! -L "$dmg_path" && -s "$dmg_path" ]] \
  || fail "the DMG must be a non-empty regular file."
[[ -f "$archive_path" && ! -L "$archive_path" && -s "$archive_path" ]] \
  || fail "the updater archive must be a non-empty regular file."
[[ -f "$signature_path" && ! -L "$signature_path" && -s "$signature_path" ]] \
  || fail "the updater signature must be a non-empty regular file."
[[ -f "$provenance_path" && ! -L "$provenance_path" && -s "$provenance_path" ]] \
  || fail "the macOS release provenance file is missing; rebuild with release:macos:local."
[[ "$(basename -- "$dmg_path")" == *"_${version}_"* ]] \
  || fail "the DMG filename does not contain release version '$version'."

temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/onyx-macos-publish.XXXXXX")" \
  || fail "could not create a private temporary directory."
cleanup() {
  rm -rf -- "$temporary_directory"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

hdiutil verify "$dmg_path" >/dev/null \
  || fail "the DMG failed hdiutil verification."
codesign --verify --deep --strict "$app_path" \
  || fail "codesign verification failed for the generated app."
spctl --assess --type execute "$app_path" \
  || fail "Gatekeeper rejected the generated app."
xcrun stapler validate "$app_path" \
  || fail "the generated app has no valid stapled notarization ticket."
spctl --assess --type open --context context:primary-signature "$dmg_path" \
  || fail "Gatekeeper rejected the generated DMG."
xcrun stapler validate "$dmg_path" \
  || fail "the generated DMG has no valid stapled notarization ticket."
gzip -t "$archive_path" \
  || fail "the updater archive failed gzip verification."
archive_listing="$temporary_directory/archive-list.txt"
tar -tzf "$archive_path" >"$archive_listing" \
  || fail "the updater archive could not be inspected."
app_plist_path="$(awk '$0 ~ "^([.]/)?[^/]+[.]app/Contents/Info[.]plist$" { print; exit }' "$archive_listing")"
[[ -n "$app_plist_path" ]] \
  || fail "the updater archive does not contain an application Info.plist."
tar -xOzf "$archive_path" "$app_plist_path" >"$temporary_directory/Info.plist" \
  || fail "the application Info.plist could not be read."
archive_version="$(plutil -extract CFBundleShortVersionString raw -o - "$temporary_directory/Info.plist" 2>/dev/null)" \
  || fail "the application Info.plist has no bundle version."
[[ "$archive_version" == "$version" ]] \
  || fail "archive version '$archive_version' does not match '$version'."

cargo run --quiet \
  --manifest-path src-tauri/Cargo.toml \
  --example verify_updater_signature \
  -- src-tauri/tauri.conf.json "$archive_path" "$signature_path" \
  || fail "the updater archive did not verify against the public key embedded in Onyx."

signature="$(LC_ALL=C tr -d '[:space:]' <"$signature_path")"
[[ "${#signature}" -ge 64 && "${#signature}" -le 8192 ]] \
  || fail "the updater signature has an invalid length."
[[ "$signature" =~ ^[A-Za-z0-9+/=]+$ ]] \
  || fail "the updater signature is not valid base64 text."

jq -e \
  --arg version "$version" \
  --arg tag "$tag" \
  --arg commit "$local_commit" \
  --arg source_repository "$source_repository" \
  --arg release_repository "$repository" \
  --arg updater_endpoint "$configured_updater_endpoint" \
  --arg target "$macos_target" \
  --arg app_file "$(basename -- "$app_path")" \
  --arg dmg_file "$(basename -- "$dmg_path")" \
  --arg dmg_sha256 "$(sha256_file "$dmg_path")" \
  --argjson dmg_bytes "$(stat -f '%z' "$dmg_path")" \
  --arg archive_file "$(basename -- "$archive_path")" \
  --arg archive_sha256 "$(sha256_file "$archive_path")" \
  --argjson archive_bytes "$(stat -f '%z' "$archive_path")" \
  --arg signature_file "$(basename -- "$signature_path")" \
  --arg signature_sha256 "$(sha256_file "$signature_path")" \
  --argjson signature_bytes "$(stat -f '%z' "$signature_path")" \
  '
    .schemaVersion == 1
    and .platform == "macos"
    and .target == $target
    and .version == $version
    and .source.repository == $source_repository
    and .source.tag == $tag
    and .source.commit == $commit
    and .source.clean == true
    and .release.repository == $release_repository
    and .release.updaterEndpoint == $updater_endpoint
    and .artifacts.app.file == $app_file
    and .artifacts.dmg == {
      file: $dmg_file, sha256: $dmg_sha256, bytes: $dmg_bytes
    }
    and .artifacts.updaterArchive == {
      file: $archive_file, sha256: $archive_sha256, bytes: $archive_bytes
    }
    and .artifacts.updaterSignature == {
      file: $signature_file,
      sha256: $signature_sha256,
      bytes: $signature_bytes
    }
  ' "$provenance_path" >/dev/null \
  || fail "macOS artifacts do not match their clean tagged-build provenance."

if ! release_notes="$(awk -v expected="$version" '
  /^##[[:space:]]+/ {
    heading = $0
    sub(/^##[[:space:]]+/, "", heading)
    split(heading, fields, /[[:space:]]+/)
    if (found) exit
    if (fields[1] == expected) {
      found = 1
      next
    }
  }
  found && !started && /^[[:space:]]*$/ { next }
  found {
    started = 1
    print
  }
  END {
    if (!found || !started) exit 2
  }
' CHANGELOG.md)"; then
  fail "CHANGELOG.md has no non-empty section for version '$version'."
fi
[[ "${#release_notes}" -le 131072 ]] \
  || fail "release notes exceed the 128 KiB manifest limit."

release_json="$(gh release view "$tag" \
  --repo "$repository" \
  --json tagName,isDraft,isPrerelease,isImmutable,url,publishedAt,assets)" \
  || fail "GitHub release '$tag' does not exist or is not accessible."
[[ "$(jq -r '.tagName' <<<"$release_json")" == "$tag" ]] \
  || fail "GitHub returned a release for an unexpected tag."
[[ "$(jq -r '.isDraft' <<<"$release_json")" == "true" ]] \
  || fail "release '$tag' must remain a draft until all platforms are verified."
[[ "$(jq -r '.isPrerelease' <<<"$release_json")" == "false" ]] \
  || fail "release '$tag' is marked as a prerelease and cannot back the latest updater endpoint."
[[ "$(jq -r '.isImmutable' <<<"$release_json")" == "false" ]] \
  || fail "release '$tag' is immutable and cannot receive the macOS assets."
[[ "$(jq -r '.publishedAt == null' <<<"$release_json")" == "true" ]] \
  || fail "draft release '$tag' unexpectedly has a publication timestamp."

latest_count="$(jq '[.assets[] | select(.name == "latest.json")] | length' <<<"$release_json")"
windows_provenance_count="$(jq '
  [.assets[] | select(.name == "onyx-windows-release-provenance.json")] | length
' <<<"$release_json")"
[[ "$latest_count" == "1" ]] \
  || fail "draft release '$tag' must contain exactly one Windows-generated latest.json."
[[ "$windows_provenance_count" == "1" ]] \
  || fail "draft release '$tag' must contain exactly one Windows build provenance asset."

base_manifest="$temporary_directory/base-latest.json"
gh release download "$tag" \
  --repo "$repository" \
  --pattern latest.json \
  --pattern onyx-windows-release-provenance.json \
  --dir "$temporary_directory" \
  --clobber >/dev/null \
  || fail "Windows updater metadata could not be downloaded."
downloaded_manifest="$temporary_directory/latest.json"
windows_provenance="$temporary_directory/onyx-windows-release-provenance.json"
[[ -f "$downloaded_manifest" && ! -L "$downloaded_manifest" ]] \
  || fail "GitHub did not return the expected latest.json file."
[[ -f "$windows_provenance" && ! -L "$windows_provenance" ]] \
  || fail "GitHub did not return the expected Windows provenance file."
[[ "$(wc -c <"$downloaded_manifest")" -le 1048576 ]] \
  || fail "existing latest.json exceeds the 1 MiB safety limit."
[[ "$(wc -c <"$windows_provenance")" -le 1048576 ]] \
  || fail "Windows provenance exceeds the 1 MiB safety limit."

encoded_tag="$(jq -nr --arg value "$tag" '$value | @uri')"
download_prefix="https://github.com/$repository/releases/download/$encoded_tag/"
jq -e \
  --arg version "$version" \
  --arg prefix "$download_prefix" \
  '
    type == "object"
    and .version == $version
    and (.pub_date | type == "string" and length > 0)
    and (.platforms | type == "object")
    and (.platforms["windows-x86_64"] |
      type == "object"
      and (.url | type == "string" and startswith($prefix))
      and (.signature | type == "string" and length > 0)
    )
    and ([.platforms[] |
      type == "object"
      and (.url | type == "string" and startswith($prefix))
      and (.signature | type == "string" and length > 0)
    ] | all)
  ' "$downloaded_manifest" >/dev/null \
  || fail "latest.json has no complete Windows x86_64 updater entry for this release."
cp "$downloaded_manifest" "$base_manifest"
published_at="$(jq -er '.pub_date' "$base_manifest")"

windows_url="$(jq -er '.platforms["windows-x86_64"].url' "$base_manifest")"
windows_signature="$(jq -er '.platforms["windows-x86_64"].signature' "$base_manifest")"
windows_asset_name="$(jq -er \
  --arg url "$windows_url" \
  '[.assets[] | select(.url == $url) | .name] | select(length == 1) | .[0]' \
  <<<"$release_json")" \
  || fail "the Windows updater URL does not identify exactly one release asset."
[[ "$windows_asset_name" =~ ^[A-Za-z0-9_.+-]+$ ]] \
  || fail "the Windows updater asset has an unsafe filename."
windows_directory="$temporary_directory/windows"
mkdir "$windows_directory"
gh release download "$tag" \
  --repo "$repository" \
  --pattern "$windows_asset_name" \
  --dir "$windows_directory" \
  --clobber >/dev/null \
  || fail "the Windows updater archive could not be downloaded."
windows_archive="$windows_directory/$windows_asset_name"
[[ -f "$windows_archive" && ! -L "$windows_archive" && -s "$windows_archive" ]] \
  || fail "the Windows updater archive is missing after download."
printf '%s\n' "$windows_signature" >"$temporary_directory/windows-updater.sig"
cargo run --quiet \
  --manifest-path src-tauri/Cargo.toml \
  --example verify_updater_signature \
  -- src-tauri/tauri.conf.json \
  "$windows_archive" "$temporary_directory/windows-updater.sig" \
  || fail "the Windows updater archive did not verify against the embedded public key."

jq -e \
  --arg version "$version" \
  --arg tag "$tag" \
  --arg commit "$local_commit" \
  --arg source_repository "$source_repository" \
  --arg release_repository "$repository" \
  --arg updater_endpoint "$configured_updater_endpoint" \
  --arg asset "$windows_asset_name" \
  --arg sha256 "$(sha256_file "$windows_archive")" \
  --argjson bytes "$(stat -f '%z' "$windows_archive")" \
  '
    .schemaVersion == 1
    and .platform == "windows"
    and .version == $version
    and .source.repository == $source_repository
    and .source.tag == $tag
    and .source.commit == $commit
    and .source.clean == true
    and .release.repository == $release_repository
    and .release.updaterEndpoint == $updater_endpoint
    and ([.artifacts[] |
      select(.file == $asset and .sha256 == $sha256 and .bytes == $bytes)
    ] | length == 1)
  ' "$windows_provenance" >/dev/null \
  || fail "the Windows updater does not match its clean tagged-build provenance."

upload_archive="$temporary_directory/Onyx_${version}_${target_slug}.app.tar.gz"
upload_signature="$upload_archive.sig"
upload_provenance="$temporary_directory/Onyx_${version}_${target_slug}.provenance.json"
cp "$archive_path" "$upload_archive"
cp "$signature_path" "$upload_signature"
cp "$provenance_path" "$upload_provenance"

encoded_archive_name="$(jq -nr --arg value "$(basename -- "$upload_archive")" '$value | @uri')"
archive_url="https://github.com/$repository/releases/download/$encoded_tag/$encoded_archive_name"
macos_platforms='{}'
for platform_key in "${platform_keys[@]}"; do
  macos_platforms="$(jq -cn \
    --argjson current "$macos_platforms" \
    --arg platform "$platform_key" \
    --arg signature "$signature" \
    --arg url "$archive_url" \
    '$current + {($platform): {signature: $signature, url: $url}}')"
done

manifest_path="$temporary_directory/generated-latest.json"
jq \
  --arg version "$version" \
  --arg notes "$release_notes" \
  --arg pub_date "$published_at" \
  --argjson macos "$macos_platforms" \
  '
    .version = $version
    | .notes = $notes
    | .pub_date = $pub_date
    | .platforms = ((.platforms // {}) + $macos)
  ' "$base_manifest" >"$manifest_path" \
  || fail "could not generate the merged updater manifest."

download_prefix="https://github.com/$repository/releases/download/$encoded_tag/"
jq -e \
  --arg version "$version" \
  --arg notes "$release_notes" \
  --arg prefix "$download_prefix" \
  --argjson macos "$macos_platforms" \
  --argjson windows "$(jq '.platforms[\"windows-x86_64\"]' "$base_manifest")" \
  '
    . as $manifest
    | type == "object"
      and .version == $version
      and .notes == $notes
      and (.pub_date | type == "string" and length > 0)
      and (.platforms | type == "object" and length > 0)
      and ([.platforms[] |
        type == "object"
        and (.url | type == "string" and startswith($prefix))
        and (.signature | type == "string" and length > 0)
      ] | all)
      and $manifest.platforms["windows-x86_64"] == $windows
      and ([$macos | to_entries[] |
        $manifest.platforms[.key] == .value
      ] | all)
  ' "$manifest_path" >/dev/null \
  || fail "the generated updater manifest failed validation."

printf 'Release: %s (%s)\n' "$tag" "$repository"
printf 'Target: %s\n' "$macos_target"
printf 'DMG: %s\n' "$(basename -- "$dmg_path")"
printf 'Updater archive: %s\n' "$(basename -- "$upload_archive")"
printf 'Updater platforms: %s\n' "${platform_keys[*]}"

if [[ "$dry_run" == true ]]; then
  printf 'Dry run complete; no GitHub assets were changed.\n'
  printf 'The draft would receive macOS assets and publish only after final verification.\n'
  jq '{version, pub_date, platforms: (.platforms | keys), notes}' "$manifest_path"
  exit 0
fi

gh release upload "$tag" \
  "$dmg_path" \
  "$upload_archive" \
  "$upload_signature" \
  "$upload_provenance" \
  --repo "$repository" \
  --clobber \
  || fail "macOS release assets could not be uploaded."

uploaded_assets="$(gh release view "$tag" --repo "$repository" --json assets)" \
  || fail "uploaded release assets could not be verified."
for asset_name in \
  "$(basename -- "$dmg_path")" \
  "$(basename -- "$upload_archive")" \
  "$(basename -- "$upload_signature")" \
  "$(basename -- "$upload_provenance")"; do
  jq -e --arg name "$asset_name" \
    '[.assets[] | select(.name == $name)] | length == 1' \
    <<<"$uploaded_assets" >/dev/null \
    || fail "uploaded asset '$asset_name' is missing or ambiguous."
done

cp "$manifest_path" "$temporary_directory/latest.json"
gh release upload "$tag" \
  "$temporary_directory/latest.json" \
  --repo "$repository" \
  --clobber \
  || fail "latest.json could not be uploaded."

verification_directory="$temporary_directory/verify"
mkdir "$verification_directory"
gh release download "$tag" \
  --repo "$repository" \
  --pattern latest.json \
  --dir "$verification_directory" \
  --clobber >/dev/null \
  || fail "the published latest.json could not be downloaded for verification."
cmp -s "$temporary_directory/latest.json" "$verification_directory/latest.json" \
  || fail "the published latest.json differs from the validated local manifest."

verification_assets_directory="$temporary_directory/verify-assets"
mkdir "$verification_assets_directory"
for asset_name in \
  "$(basename -- "$dmg_path")" \
  "$(basename -- "$upload_archive")" \
  "$(basename -- "$upload_signature")" \
  "$(basename -- "$upload_provenance")"; do
  gh release download "$tag" \
    --repo "$repository" \
    --pattern "$asset_name" \
    --dir "$verification_assets_directory" \
    --clobber >/dev/null \
    || fail "uploaded asset '$asset_name' could not be downloaded for verification."
done
cmp -s "$dmg_path" "$verification_assets_directory/$(basename -- "$dmg_path")" \
  || fail "the uploaded DMG differs from the validated local artifact."
cmp -s "$upload_archive" "$verification_assets_directory/$(basename -- "$upload_archive")" \
  || fail "the uploaded updater archive differs from the validated local artifact."
cmp -s "$upload_signature" "$verification_assets_directory/$(basename -- "$upload_signature")" \
  || fail "the uploaded updater signature differs from the validated local artifact."
cmp -s "$upload_provenance" "$verification_assets_directory/$(basename -- "$upload_provenance")" \
  || fail "the uploaded provenance differs from the validated local metadata."

release_notes_path="$temporary_directory/release-notes.md"
printf '%s\n' "$release_notes" >"$release_notes_path"
gh release edit "$tag" \
  --repo "$repository" \
  --notes-file "$release_notes_path" >/dev/null \
  || fail "the GitHub release body could not be updated from CHANGELOG.md."
published_body="$(gh release view "$tag" --repo "$repository" --json body --jq '.body')" \
  || fail "the updated GitHub release body could not be verified."
[[ "$published_body" == "$release_notes" ]] \
  || fail "the published GitHub release body differs from CHANGELOG.md."

prepublication_json="$(gh release view "$tag" \
  --repo "$repository" \
  --json isDraft,isPrerelease,assets)" \
  || fail "the fully assembled draft could not be inspected."
[[ "$(jq -r '.isDraft' <<<"$prepublication_json")" == "true" ]] \
  || fail "the release stopped being a draft before final publication."
[[ "$(jq -r '.isPrerelease' <<<"$prepublication_json")" == "false" ]] \
  || fail "the release became a prerelease before final publication."
for required_asset in \
  latest.json \
  onyx-windows-release-provenance.json \
  "$windows_asset_name" \
  "$(basename -- "$dmg_path")" \
  "$(basename -- "$upload_archive")" \
  "$(basename -- "$upload_signature")" \
  "$(basename -- "$upload_provenance")"; do
  jq -e --arg name "$required_asset" \
    '[.assets[] | select(.name == $name)] | length == 1' \
    <<<"$prepublication_json" >/dev/null \
    || fail "the fully assembled draft is missing exactly one '$required_asset' asset."
done

gh release edit "$tag" \
  --repo "$repository" \
  --draft=false >/dev/null \
  || fail "all assets are valid, but GitHub could not publish the draft release."
final_release_json="$(gh release view "$tag" \
  --repo "$repository" \
  --json isDraft,isPrerelease,publishedAt,url)" \
  || fail "the published release could not be inspected."
[[ "$(jq -r '.isDraft' <<<"$final_release_json")" == "false" ]] \
  || fail "GitHub still reports '$tag' as a draft."
[[ "$(jq -r '.isPrerelease' <<<"$final_release_json")" == "false" ]] \
  || fail "GitHub reports '$tag' as a prerelease."
jq -e '.publishedAt | type == "string" and length > 0' \
  <<<"$final_release_json" >/dev/null \
  || fail "the release has no publication timestamp."

printf 'Published the verified multi-platform release: %s\n' \
  "$(jq -r '.url' <<<"$final_release_json")"
