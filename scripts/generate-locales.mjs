import { readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { parse } from '@babel/parser'
import { Converter } from 'opencc-js/cn2t'

const convert = Converter({ from: 'cn', to: 'tw' })
const translations = new Map()
function collect(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) collect(path)
    else if (/\.tsx?$/.test(entry.name)) {
      const source = parse(readFileSync(path, 'utf8'), {
        sourceFilename: path,
        sourceType: 'module',
        plugins: ['typescript', 'jsx'],
        attachComment: false,
      })
      const visit = (node) => {
        if (!node || typeof node !== 'object') return
        if (node.type === 'StringLiteral' && /[\u3400-\u9fff]/.test(node.value))
          translations.set(node.value, convert(node.value))
        for (const value of Object.values(node)) {
          if (Array.isArray(value)) value.forEach(visit)
          else if (value && typeof value === 'object' && 'type' in value) visit(value)
        }
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
