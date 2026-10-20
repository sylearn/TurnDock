<script setup lang="ts">
import { onMounted } from 'vue'
import InboxView from './components/InboxView.vue'
import QuickView from './components/QuickView.vue'
import { start, store } from './lib/store'

const quick = window.location.hash === '#quick'
document.documentElement.classList.toggle('is-quick', quick)
onMounted(() => void start())
</script>

<template>
  <QuickView v-if="quick" />
  <InboxView v-else />
  <Transition name="toast">
    <p v-if="store.toast" class="toast" role="status">{{ store.toast }}</p>
  </Transition>
</template>

<style>
.toast {
  position: fixed;
  left: 50%;
  bottom: 20px;
  z-index: 10;
  margin: 0;
  padding: 8px 14px;
  border-radius: 8px;
  background: var(--ink);
  color: var(--sheet);
  font-size: 13px;
  box-shadow: var(--shade);
  transform: translateX(-50%);
  pointer-events: none;
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 160ms var(--ease), transform 160ms var(--ease);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, 6px);
}
</style>
