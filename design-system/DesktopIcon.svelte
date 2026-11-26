<!-- SPDX-License-Identifier: MPL-2.0 -->
<script lang="ts">
  const icons = {
    add: ['\uE710', 'M8 3v10M3 8h10'],
    preview: ['\uE8FF', 'M3 2.5h7l3 3v8H3zM10 2.5v3h3M5.5 8h5M5.5 10.5h4'],
    search: ['\uE721', 'M10.5 10.5l3.5 3.5M12 7a5 5 0 1 1-10 0 5 5 0 0 1 10 0Z'],
    library: ['\uE7B8', 'M2.5 2.5h3v11h-3zM7 2.5h3v11H7zM11.5 3l2.5-.5 2 10.5-2.5.5z'],
    review: ['\uE90F', 'm6 9-4 4 1 1 4-4a4.5 4.5 0 0 0 6.5-5.5L11 7l-2-2 2.5-2.5A4.5 4.5 0 0 0 6 9Z'],
    back: ['\uE72B', 'M13 8H3m0 0 4-4M3 8l4 4'],
    play: ['\uE768', 'M5 2.5 13 8l-8 5.5z'],
    account: ['\uE77B', 'M11 5a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm-9 9a6 6 0 0 1 12 0z'],
    done: ['\uE73E', 'm3 8 3.1 3.3L13 4.4'],
    pending: ['\uE91F', 'M13 8a5 5 0 1 1-10 0 5 5 0 0 1 10 0Z'],
    blocked: ['\uE7BA', 'm8 2-6 12h12zM8 6v3m0 2v.5']
  } as const
  const {name}: {name: keyof typeof icons} = $props()
</script>

<span class="desktop-icon" aria-hidden="true">
  <span class="desktop-icon-native">{icons[name][0]}</span>
  <svg class="desktop-icon-fallback" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round">
    <path d={icons[name][1]} />
  </svg>
</span>
