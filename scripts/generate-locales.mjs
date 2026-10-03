import { readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import ts from 'typescript'
import { Converter } from 'opencc-js/cn2t'

const convert = Converter({ from: 'cn', to: 'tw' })
const translations = new Map()
function collect(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) collect(path)
    else if (/\.tsx?$/.test(entry.name)) {
      const source = ts.createSourceFile(
        path,
        readFileSync(path, 'utf8'),
        ts.ScriptTarget.Latest,
        true,
      )
      const visit = (node) => {
        if (ts.isStringLiteral(node) && /[\u3400-\u9fff]/.test(node.text))
          translations.set(node.text, convert(node.text))
        ts.forEachChild(node, visit)
      }
      visit(source)
    }
  }
}
collect('src')
mkdirSync('src/locales', { recursive: true })
writeFileSync(
  'src/locales/zh-TW.json',
  JSON.stringify(
    Object.fromEntries([...translations].sort(([a], [b]) => a.localeCompare(b, 'en'))),
    null,
    2,
  ) + '\n',
)
