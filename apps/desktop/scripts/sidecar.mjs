// 打包前构建 turndock 命令，放到 src-tauri/binaries/ 供 bundle.externalBin 使用。
// 应用包里的 Contents/MacOS/turndock 就来自这里，随应用一起更新。
// 目标取自 Tauri 注入的 TAURI_ENV_TARGET_TRIPLE；universal 构建会把两个架构合并成一个文件。
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const desktop = join(dirname(fileURLToPath(import.meta.url)), '..')
const root = join(desktop, '..', '..')
const out = join(desktop, 'src-tauri', 'binaries')
const host = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/host: (\S+)/)[1]
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || host
const debug = process.env.TAURI_ENV_DEBUG === 'true'
const profile = debug ? 'debug' : 'release'

function build(target) {
  const args = ['build', '-p', 'turndock-bridge', '--locked', '--target', target]
  if (!debug) args.push('--release')
  execFileSync('cargo', args, { cwd: root, stdio: 'inherit' })
  return join(root, 'target', target, profile, 'turndock')
}

mkdirSync(out, { recursive: true })
const dest = (t) => join(out, `turndock-${t}`)

if (triple === 'universal-apple-darwin') {
  const arm = build('aarch64-apple-darwin')
  const x86 = build('x86_64-apple-darwin')
  execFileSync('lipo', ['-create', '-output', dest(triple), arm, x86], { stdio: 'inherit' })
  // 分架构编译主程序时，tauri-build 也会按各自的 triple 查找。
  copyFileSync(arm, dest('aarch64-apple-darwin'))
  copyFileSync(x86, dest('x86_64-apple-darwin'))
} else {
  copyFileSync(build(triple), dest(triple))
}
console.log(`turndock → ${dest(triple)}`)
