<script module lang="ts">
  import icons from '../design-system/instance-icons.json'
  const paths = new Map(icons.map(icon => [icon.id, icon.rows.flatMap((row, y) =>
    Array.from({length: 12}, (_, x) => row & (1 << (11 - x)) ? `M${x} ${y}h1v1h-1z` : '')
  ).join('')]))
</script>

<script lang="ts">
  import type { PersonalProfile } from './personal'
  let { profile }: { profile: PersonalProfile } = $props()
  let path = $derived(paths.get(profile.iconId ?? '') ?? paths.get(profile.importedPack ? 'pack' : 'block'))
</script>

<svg viewBox="0 0 12 12" fill="currentColor" aria-hidden="true" shape-rendering="crispEdges">
  <path d={path} />
</svg>
