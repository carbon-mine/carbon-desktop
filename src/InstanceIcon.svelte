<script module lang="ts">
  const images = import.meta.glob<string>('../design-system/instance-icons/*.png', {
    eager: true, import: 'default', query: '?url',
  })
  const sprites = new Map(Object.entries(images).map(([path, url]) => [
    path.slice(path.lastIndexOf('/') + 1, -4), url,
  ]))
</script>

<script lang="ts">
  import type { PersonalProfile } from './personal'
  let { profile }: { profile: PersonalProfile } = $props()
  let src = $derived(sprites.get(profile.iconId ?? '') ?? sprites.get(profile.importedPack ? 'pack' : 'block'))
</script>

<img alt="" {src} class="instance-sprite" />
