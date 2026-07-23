/** Normalize user-entered preview addresses without allowing privileged schemes. */
export function normalizeBrowserUrl(input: string) {
  const value = input.trim()
  if (!value) throw new Error("Enter a URL to open.")
  const localPreview = /^(localhost|127\.0\.0\.1|\[::1\])(?::\d+)?(?:\/|$)/i.test(value)
  const withProtocol = localPreview
    ? `http://${value}`
    : /^[a-z][a-z\d+.-]*:/i.test(value)
      ? value
      : `https://${value}`
  const url = new URL(withProtocol)
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Browser tabs support HTTP and HTTPS URLs only.")
  }
  return url.toString()
}
