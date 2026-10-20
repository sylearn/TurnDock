// 从 Rust 类型导出 TypeScript 定义（ts-rs），写入 src/generated。
import { execFileSync } from 'node:child_process'
import { rmSync, mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const out = resolve(here, '../src/generated')
rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })
for (const pkg of ['turndock-core', 'turndock-broker']) {
  execFileSync('cargo', ['test', '-q', '-p', pkg, '--features', 'ts', '--lib', 'export_bindings'], {
    stdio: 'inherit',
    cwd: resolve(here, '../../..'),
    env: { ...process.env, TS_RS_EXPORT_DIR: out },
  })
}
