<!-- SPDX-License-Identifier: MPL-2.0 -->
<script lang="ts">
  import { untrack } from 'svelte'
  import { prefersReducedMotion } from 'svelte/motion'
  import { kib } from './library'

  let { mode, label, done = 0, total = 0, bytes = 0, totalBytes = 0, frozen = false }: {
    mode: 'bytes' | 'files' | 'busy'
    label: string
    done?: number
    total?: number
    bytes?: number
    totalBytes?: number
    frozen?: boolean
  } = $props()

  let shown = $state({ count: 0, bytes: 0 })
  let previous: { count: number; bytes: number; total: number; totalBytes: number } | undefined

  $effect(() => {
    const target = { count: done, bytes }
    const reduced = prefersReducedMotion.current
    const snap = !previous || mode === 'busy' || frozen || reduced || done >= total
      || total !== previous.total || totalBytes !== previous.totalBytes
      || done < previous.count || bytes < previous.bytes
    previous = { ...target, total, totalBytes }
    const from = untrack(() => shown)
    if (snap || (from.count === done && from.bytes === bytes)) {
      shown = target
      return
    }

    const started = performance.now()
    let frame = requestAnimationFrame(advance)
    function advance(now: number) {
      const t = Math.min(1, Math.max(0, (now - started) / 160))
      const eased = 1 - (1 - t) ** 3
      if (t >= 1) {
        shown = target
      } else {
        shown = {
          count: from.count + (target.count - from.count) * eased,
          bytes: from.bytes + (target.bytes - from.bytes) * eased
        }
        frame = requestAnimationFrame(advance)
      }
    }
    return () => cancelAnimationFrame(frame)
  })

  function counter(count: number, received: number) {
    return mode === 'bytes' ? kib(count) + ' / ' + kib(total)
      : Math.floor(count) + ' / ' + total + ' files · ' + kib(received) + ' / ' + kib(totalBytes)
  }
</script>

<div class="transfer">
  {#if mode === 'busy'}
    <progress class="semantic" aria-label={label}></progress>
    <div class="rail busy" class:frozen aria-hidden="true"><span></span></div>
  {:else}
    <progress class="semantic" aria-label={label} value={done} max={total} aria-valuetext={counter(done, bytes)}></progress>
    <div class="rail" aria-hidden="true">
      <span style:transform={'scaleX(' + Math.min(1, Math.max(0, shown.count / total)) + ')'}></span>
    </div>
    <p class="technical" aria-hidden="true">{counter(shown.count, shown.bytes)}</p>
  {/if}
</div>

<style>
  .transfer { position: relative; max-inline-size: 28rem; margin-block-start: 0.65rem; }
  .semantic { position: absolute; inline-size: 1px; block-size: 1px; clip-path: inset(50%); }
  .rail { block-size: 4px; overflow: hidden; background: var(--desktop-line); border-radius: 2px; }
  .rail span { display: block; block-size: 100%; background: var(--desktop-action); transform-origin: left; }
  .technical { margin-block: 0.25rem 0; font-variant-numeric: tabular-nums; }
  .busy span { inline-size: 30%; animation: travel 1.35s ease-in-out infinite alternate; }
  @keyframes travel { from { transform: translateX(0); } to { transform: translateX(233%); } }
  .frozen span { animation: none; transform: translateX(116%); }
  @media (prefers-reduced-motion: reduce) {
    .busy span { animation: none; transform: translateX(116%); }
  }
  @media (forced-colors: active) {
    .rail { forced-color-adjust: none; background: Canvas; outline: 1px solid CanvasText; }
    .rail span { background: Highlight; }
  }
</style>
