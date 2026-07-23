export type OnyxColorScheme = "system" | "light" | "dark"

export function storedColorScheme(): OnyxColorScheme {
  const value = localStorage.getItem("onyx.color-scheme") ?? localStorage.getItem("zai.color-scheme")
  return value === "light" || value === "dark" ? value : "system"
}

export function applyDocumentTheme(preference: OnyxColorScheme = storedColorScheme()) {
  const resolved = preference === "system"
    ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
    : preference
  document.documentElement.dataset.colorScheme = resolved
  document.documentElement.dataset.theme = "oc-2"
  document.body.dataset.newLayout = ""
  return resolved
}
