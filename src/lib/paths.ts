/** Format Windows verbatim paths for display without changing filesystem identity. */
export function displayPath(path: string): string {
  if (path.startsWith('\\\\?\\UNC\\')) return `\\\\${path.slice(8)}`
  if (/^\\\\\?\\[a-z]:\\/i.test(path)) return path.slice(4)
  return path
}

/** Normalize paths embedded in messages, leaving filesystem inputs untouched. */
export function displayDiagnostic(message: string): string {
  return message.replace(/\\\\\?\\UNC\\/gi, '\\\\').replace(/\\\\\?\\(?=[a-z]:\\)/gi, '')
}
