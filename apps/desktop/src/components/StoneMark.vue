<script setup lang="ts">
// 状态点：绿色实心 = 轮到你且 AI 在等；琥珀 = 轮到你但 AI 没在等；紫色空心 = 已交还；虚线 = 已结束。
import type { Stone } from '../lib/format'

defineProps<{ stone: Stone; size?: number }>()
</script>

<template>
  <svg
    class="stone"
    :class="stone"
    :width="size ?? 12"
    :height="size ?? 12"
    viewBox="0 0 12 12"
    aria-hidden="true"
  >
    <circle v-if="stone === 'mine'" cx="6" cy="6" r="5.5" />
    <template v-else-if="stone === 'idle'">
      <circle cx="6" cy="6" r="5.25" class="ring warn" />
      <circle cx="6" cy="6" r="3" />
    </template>
    <circle v-else cx="6" cy="6" r="4.75" class="ring" />
  </svg>
</template>

<style scoped>
.stone {
  flex: none;
  fill: var(--ink);
}
/* 轮到你且 AI 在等：绿色“在线”点 */
.stone.mine {
  fill: var(--ok);
}
.stone.idle {
  fill: var(--warn);
}
.ring {
  fill: none;
  stroke: var(--signal);
  stroke-width: 1.5;
}
.ring.warn {
  stroke: var(--warn);
}
.closed .ring {
  stroke: var(--edge);
  stroke-dasharray: 2 2;
}
</style>
