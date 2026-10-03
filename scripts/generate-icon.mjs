import { writeFileSync, mkdirSync } from 'node:fs'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { Terminal } from 'lucide-react'

const terminal = renderToStaticMarkup(
  createElement(Terminal, {
    width: 512,
    height: 512,
    x: 256,
    y: 256,
    stroke: '#fafafa',
    strokeWidth: 1.8,
  }),
)
mkdirSync('src-tauri/icons', { recursive: true })
writeFileSync(
  'src-tauri/icons/source.svg',
  `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><rect x="48" y="48" width="928" height="928" rx="232" fill="#171717"/>${terminal}</svg>`,
)
