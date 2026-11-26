<!-- SPDX-License-Identifier: MPL-2.0 -->
# World maps in the instance library

Open `index.html` locally. All terrain and locations in this study are original synthetic fixtures. The library contains a general instance with multiple destinations and a dedicated server shortcut. Opening a map never records a visit. A real visit observer and a qualified offline tile reader are tracked separately.

## Carbon Panel reference

Panel places an explicit **Expand world map** control over the thumbnail. It opens a full-size dialog on the same server route. Closing removes the map-specific query parameters while preserving unrelated page state. Its modal handles Escape and focus return. The viewport supports keyboard and pointer navigation and uses pixelated imagery. These interactions carry over to the launcher. Desktop adds an independent, nonmodal map window so a player can keep a map on another monitor. [Panel workspace](https://github.com/athNdev/carbonpanel/blob/c8adcfe6af711498074884172a188115ed5a3129/web/carbon-panel/src/lib/components/worldview/WorldViewWorkspace.svelte) · [Server route](https://github.com/athNdev/carbonpanel/blob/c8adcfe6af711498074884172a188115ed5a3129/web/carbon-panel/src/routes/servers/%5Bid%5D/%2Bpage.svelte) · [Modal](https://github.com/athNdev/carbonpanel/blob/c8adcfe6af711498074884172a188115ed5a3129/web/carbon-panel/src/lib/components/carbon/CarbonModal.svelte)

Arrow keys pan a focused map. Plus and minus change zoom. Keep a visible Close control, trap focus during expansion and return focus to its opener. A separate window owns its own camera and selection. These choices follow documented map and dialog conventions. [Leaflet keyboard map](https://leafletjs.com/reference.html#map-keyboard) · [WAI dialog interaction](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)

| Composition | Player sees | Review question |
| --- | --- | --- |
| Scenic | Wide terrain crop above an opaque identity and action area | Is the village or river recognizable at tile size? |
| Split | Smaller crop alongside the instance details | Can long instance and world names coexist with Join at the middle width? |
| Compact | Icon or narrow thumbnail at the leading edge | Does the action stay in a predictable place when imagery disappears? |

The provisional scenic layout separates labels from terrain contrast. At compact widths, the icon preserves identity without squeezing the world name and action. Center imagery on a recorded position in the selected world and dimension when known. Otherwise center the available coverage and omit the location marker. Unsaved coverage remains visibly empty.

## World ownership and evidence

A launcher instance can hold several saves and saved multiplayer destinations. A dedicated server binding records where Join should connect. Browsing any of them never changes the last entered world. Only a qualified observation from a supervised game run may advance the preview. Key terrain by stable instance, destination, known logical world, dimension and layer. A proxied server with an ambiguous subworld can supply a visit name while its map stays unavailable.

Six sampled Minecraft builds declare a passive Quick Play log separately from automatic connection. Release 1.19.4 lacks this argument. Real in-game transitions, including two joins from the in-game menu in one run, require qualification per version and loader. A synthetic read-only test places region images and leaves missing or damaged regions empty without altering the source. A current producer-generated JourneyMap sample still needs verification. [World-entry specification](https://www.minecraft.net/en-us/article/minecraft-snapshot-23w14a) · [JourneyMap stitcher](https://github.com/TeamJM/journeymap-tools/blob/7e7b0ad770967ae3b0cf23656ed2badda62f4ac3/src/main/kotlin/info/journeymap/tools/images/ImageStitcher.kt)

## Review the interaction

Open Alder Reach from its tile, pan and zoom, then return with Escape and check keyboard focus. Open the same world in a separate window. Change the main window to another dimension and verify the second window keeps its selection. Compare a partial map and an empty map at normal and narrow widths. Try reduced motion. The product owner chooses the tile composition at the visual review.

[Mixed destinations](https://carbonpanel.atlassian.net/browse/CDESK-83) · [Entry observer](https://carbonpanel.atlassian.net/browse/CDESK-84) · [Tile reader](https://carbonpanel.atlassian.net/browse/CDESK-85) · [Design review](https://carbonpanel.atlassian.net/browse/CDESK-86)
