// 键盘模型（docs/02 第 2 节）。纯函数，便于测试。

export type KeyAction =
  | { kind: 'send' }
  | { kind: 'quick'; index: number }
  | { kind: 'option'; index: number }
  | { kind: 'next' }
  | { kind: 'prev' }
  | { kind: 'later' }
  | { kind: 'decline' }
  | { kind: 'search' }
  | { kind: 'toggleView' }
  | { kind: 'escape' }

export interface KeyLike {
  key: string
  code: string
  metaKey: boolean
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  isComposing?: boolean
  keyCode?: number
}

export function resolveKey(e: KeyLike, inField: boolean): KeyAction | null {
  // 输入法合成期间的任何按键都只属于输入法。
  if (e.isComposing || e.keyCode === 229) return null
  const mod = e.metaKey || e.ctrlKey
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key

  if (mod && !e.altKey && key === 'Enter') return { kind: 'send' }
  // macOS 上 ⌥1 产生的 key 是“¡”，因此用物理键 code 判断。
  const digit = /^Digit([1-9])$/.exec(e.code)
  if (e.altKey && !mod && digit) return { kind: 'quick', index: Number(digit[1]) - 1 }
  if (mod && !e.shiftKey && !e.altKey && key === 'l') return { kind: 'later' }
  if (mod && e.shiftKey && (key === 'Backspace' || key === 'Delete')) return { kind: 'decline' }
  if (mod && !e.shiftKey && !e.altKey && key === 'e') return { kind: 'toggleView' }
  if (key === 'Escape') return { kind: 'escape' }

  if (inField || mod || e.altKey) return null
  if (digit && !e.shiftKey) return { kind: 'option', index: Number(digit[1]) - 1 }
  if (key === 'j' || key === 'ArrowDown') return { kind: 'next' }
  if (key === 'k' || key === 'ArrowUp') return { kind: 'prev' }
  if (key === '/') return { kind: 'search' }
  return null
}

export function isField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
}
