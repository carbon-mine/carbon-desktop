<!-- SPDX-License-Identifier: MPL-2.0 -->
<script lang="ts">
  import type { PersonalProfile } from './personal'

  let { profile, editable, error, onEdit, onCopy, onReload }: {
    profile: PersonalProfile
    editable: boolean
    error: string
    onEdit: (opener: HTMLButtonElement) => void
    onCopy: () => void
    onReload: () => void
  } = $props()
</script>

<section class="servers" aria-label="Multiplayer">
  <h3>Multiplayer</h3>
  {#if profile.serverAddress}
    <dl class="server-inventory">
      <div><dt>Server address</dt><dd class="server-address">{profile.serverAddress}</dd></div>
      <div><dt>Local pack</dt><dd>
        {#if profile.importedPack}
          {profile.importedPack.name} · Minecraft {profile.version}
          {#if profile.importedPack.loader} · {profile.importedPack.loader}{/if}
          · {profile.importedPack.pendingFiles} pending {profile.importedPack.pendingFiles === 1 ? 'file' : 'files'}
        {:else}
          Vanilla {profile.version}
        {/if}
      </dd></div>
    </dl>
    <p class="section-help">{profile.serverJoinMessage}</p>
    <p class="section-help">Copy the address and paste it in Multiplayer in a compatible Minecraft installation.</p>
  {:else}
    <p class="section-help">Set a server address to keep it with this instance.</p>
  {/if}
  {#if error}
    <p class="task-error" role="alert">{error}</p>
  {/if}
  <div class="actions">
    {#if profile.serverAddress}
      <button onclick={onCopy}>Copy server address</button>
    {/if}
    <button disabled={!editable} onclick={event => onEdit(event.currentTarget)}>
      {profile.serverAddress ? 'Edit server address' : 'Set server address'}
    </button>
    {#if error}
      <button disabled={!editable} onclick={onReload}>Reload library</button>
    {/if}
  </div>
</section>

<style>
  .servers { border-block-start: 1px solid var(--desktop-line); margin-block-start: 1rem; padding-block-start: 0.75rem; }
  .servers h3 { margin-block: 0 0.45rem; }
  .server-inventory { margin-block: 0.5rem; display: grid; gap: 0.25rem; }
  .server-inventory div { display: grid; grid-template-columns: 7.75rem minmax(0, 1fr); gap: 0.5rem; }
  .server-inventory dt { color: var(--desktop-muted); }
  .server-inventory dd { margin: 0; overflow-wrap: anywhere; }
  .server-address { user-select: text; font-variant-numeric: tabular-nums; }
  @media (max-width: 28rem) {
    .server-inventory div { grid-template-columns: minmax(0, 1fr); gap: 0.08rem; }
  }
</style>
