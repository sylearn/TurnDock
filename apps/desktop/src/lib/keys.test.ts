import { describe, expect, it } from 'vitest'
import { resolveKey, type KeyLike } from './keys'

const k = (over: Partial<KeyLike>): KeyLike => ({
  key: '',
  code: '',
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...over,
})

describe('resolveKey', () => {
  it('输入法合成期间不触发任何动作（U01）', () => {
    expect(resolveKey(k({ key: 'Enter', metaKey: true, isComposing: true }), true)).toBeNull()
    expect(resolveKey(k({ key: 'Process', keyCode: 229 }), true)).toBeNull()
  })

  it('⌘↵ 发送，单独回车不发送', () => {
    expect(resolveKey(k({ key: 'Enter', metaKey: true }), true)).toEqual({ kind: 'send' })
    expect(resolveKey(k({ key: 'Enter' }), true)).toBeNull()
    expect(resolveKey(k({ key: 'Enter' }), false)).toBeNull()
  })

  it('⌥数字按物理键识别快捷回复，在输入框里也可用', () => {
    expect(resolveKey(k({ key: '¡', code: 'Digit1', altKey: true }), true)).toEqual({
      kind: 'quick',
      index: 0,
    })
  })

  it('数字选项与 J/K 只在输入框外生效', () => {
    expect(resolveKey(k({ key: '2', code: 'Digit2' }), false)).toEqual({ kind: 'option', index: 1 })
    expect(resolveKey(k({ key: '2', code: 'Digit2' }), true)).toBeNull()
    expect(resolveKey(k({ key: 'j' }), false)).toEqual({ kind: 'next' })
    expect(resolveKey(k({ key: 'J', shiftKey: true }), true)).toBeNull()
  })

  it('拒绝需要 ⌘⇧⌫', () => {
    expect(resolveKey(k({ key: 'Backspace', metaKey: true }), false)).toBeNull()
    expect(resolveKey(k({ key: 'Backspace', metaKey: true, shiftKey: true }), false)).toEqual({
      kind: 'decline',
    })
  })
})
