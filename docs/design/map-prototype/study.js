/* SPDX-License-Identifier: MPL-2.0 */
'use strict';

(() => {
  const MAP_SIZE = 512;
  const ZOOM_LEVELS = [1, 2, 3, 4, 6, 8, 12, 16];
  const DEFAULT_TITLE = 'Carbon Desktop · Map study';
  const TAU = Math.PI * 2;

  const icons = {
    timberline: 'pine', 'north-crossing': 'beacon', 'redstone-workshop': 'gear', 'field-notes': 'ember'
  };

  const FULL_COVERAGE = [{ x: 0, y: 0, w: MAP_SIZE, h: MAP_SIZE }];
  const NORTH_COVERAGE = [
    { x: 0, y: 0, w: 300, h: MAP_SIZE },
    { x: 300, y: 0, w: 212, h: 238 },
    { x: 300, y: 360, w: 212, h: 152 }
  ];

  const INSTANCES = [
    {
      id: 'timberline',
      name: 'Timberline Trail',
      kind: 'Local world',
      featured: { worldId: 'alder-reach', dimId: 'overworld' },
      worlds: [
        {
          id: 'alder-reach',
          name: 'Alder Reach',
          kind: 'Local world',
          defaultDim: 'overworld',
          dims: {
            overworld: {
              label: 'Overworld',
              image: 'alder-reach.png',
              seed: 'alder-reach',
              palette: 'overworld',
              coverage: FULL_COVERAGE,
              last: { x: 261, z: 243 }
            },
            nether: {
              label: 'Nether',
              seed: 'alder-reach-nether',
              palette: 'nether',
              coverage: [],
              note: 'This world has no saved Nether terrain.'
            }
          }
        },
        {
          id: 'cedar-hollow',
          name: 'Cedar Hollow',
          kind: 'Local save',
          defaultDim: 'overworld',
          dims: {
            overworld: {
              label: 'Overworld',
              seed: 'cedar-hollow',
              palette: 'overworld',
              coverage: FULL_COVERAGE,
              last: null
            }
          }
        },
        {
          id: 'alder-smp',
          name: 'Alder Reach SMP',
          kind: 'Saved server',
          defaultDim: 'overworld',
          dims: {
            overworld: {
              label: 'Overworld',
              coverage: [],
              note: 'Saved server · no local terrain was observed.'
            }
          }
        },
        {
          id: 'stonefall',
          name: 'Stonefall Creative',
          kind: 'Saved server',
          defaultDim: 'overworld',
          dims: {
            overworld: {
              label: 'Overworld',
              coverage: [],
              note: 'Saved server · no local terrain was observed.'
            }
          }
        }
      ]
    },
    {
      id: 'north-crossing',
      name: 'North Crossing',
      kind: 'Server-bound instance',
      featured: { worldId: 'north-crossing', dimId: 'overworld' },
      worlds: [
        {
          id: 'north-crossing',
          name: 'North Crossing',
          kind: 'Local world',
          defaultDim: 'overworld',
          dims: {
            overworld: {
              label: 'Overworld',
              image: 'alder-reach.png',
              seed: 'alder-reach',
              palette: 'overworld',
              coverage: NORTH_COVERAGE,
              last: null,
              note: 'Bound map · part of the terrain is missing.'
            }
          }
        }
      ]
    },
    {
      id: 'redstone-workshop',
      name: 'Redstone Workshop',
      kind: 'Local instance',
      featured: { worldId: 'redstone-workshop', dimId: 'overworld' },
      worlds: [
        {
          id: 'redstone-workshop',
          name: 'Redstone Workshop',
          kind: 'Saved server',
          defaultDim: 'overworld',
          dims: {
            overworld: {
              label: 'Overworld',
              coverage: [],
              note: 'No observed entry or saved imagery for this server.'
            },
            nether: {
              label: 'Nether',
              coverage: [],
              note: 'No observed entry or saved imagery for this server.'
            }
          }
        }
      ]
    },
    {
      id: 'field-notes',
      name: 'Field Notes',
      kind: 'Local world',
      featured: { worldId: 'field-notes', dimId: 'nether' },
      worlds: [
        {
          id: 'field-notes',
          name: 'Field Notes',
          kind: 'Local world',
          defaultDim: 'nether',
          dims: {
            nether: {
              label: 'Nether',
              image: 'ember-vault.png',
              seed: 'ember-vault',
              palette: 'nether',
              coverage: FULL_COVERAGE,
              last: null
            },
            overworld: {
              label: 'Overworld',
              coverage: [],
              note: 'No saved Overworld terrain for this world.'
            }
          }
        }
      ]
    }
  ];

  const instanceById = new Map(INSTANCES.map((instance) => [instance.id, instance]));

  const el = {
    shell: document.getElementById('shell'),
    library: document.getElementById('library'),
    mapTab: document.getElementById('mapTab'),
    cards: document.getElementById('cards'),
    libraryNav: document.getElementById('libraryNav'),
    mapNav: document.getElementById('mapNav'),
    expanded: document.getElementById('expanded'),
    viewer: document.getElementById('viewer'),
    instanceName: document.getElementById('instanceName'),
    worldTitle: document.getElementById('worldTitle'),
    expandHere: document.getElementById('expandHere'),
    detach: document.getElementById('detach'),
    dismiss: document.getElementById('dismiss'),
    instanceSelect: document.getElementById('instanceSelect'),
    worldSelect: document.getElementById('worldSelect'),
    dimSelect: document.getElementById('dimSelect'),
    largeMap: document.getElementById('largeMap'),
    emptyMap: document.getElementById('emptyMap'),
    emptyText: document.querySelector('#emptyMap span'),
    zoomIn: document.getElementById('zoomIn'),
    zoomOut: document.getElementById('zoomOut'),
    center: document.getElementById('center'),
    coverage: document.getElementById('coverage'),
    mapCoords: document.getElementById('mapCoords'),
    review: document.getElementById('review'),
    variants: document.getElementById('variants'),
    notice: document.getElementById('notice')
  };

  const state = {
    current: {
      instanceId: INSTANCES[0].id,
      worldId: INSTANCES[0].featured.worldId,
      dimId: INSTANCES[0].featured.dimId
    },
    memory: new Map(),
    dimMemory: new Map(),
    cameras: new Map(),
    variant: 'scenic',
    inlineOpen: false,
    expandedOpen: false,
    detached: false,
    expandedReturn: null,
    viewerHome: null,
    savedOverflow: '',
    drag: null,
    mapFrame: 0,
    previewFrame: 0,
    noticeTimer: 0,
    previewObserver: null,
    mapObserver: null
  };

  const imageCache = new Map();

  // ---------------------------------------------------------------------------
  // Selection and camera
  // ---------------------------------------------------------------------------

  function resolveSelection(instanceId, worldId, dimId) {
    const instance = instanceById.get(instanceId) || INSTANCES[0];
    const world = instance.worlds.find((candidate) => candidate.id === worldId) ||
      instance.worlds.find((candidate) => candidate.id === instance.featured.worldId) ||
      instance.worlds[0];
    const dims = Object.keys(world.dims);
    const dim = dimId && world.dims[dimId]
      ? dimId
      : (world.dims[world.defaultDim] ? world.defaultDim : dims[0]);
    return { inst: instance, world, dimId: dim, entry: world.dims[dim] };
  }

  function selection() {
    return resolveSelection(
      state.current.instanceId,
      state.current.worldId,
      state.current.dimId
    );
  }

  function rememberedSelection(instance) {
    const remembered = state.memory.get(instance.id);
    if (remembered) {
      return resolveSelection(instance.id, remembered.worldId, remembered.dimId);
    }
    return resolveSelection(instance.id, instance.featured.worldId, instance.featured.dimId);
  }

  function setSelection(instanceId, worldId, dimId) {
    const sel = resolveSelection(instanceId, worldId, dimId);
    state.current = {
      instanceId: sel.inst.id,
      worldId: sel.world.id,
      dimId: sel.dimId
    };
    state.memory.set(sel.inst.id, { worldId: sel.world.id, dimId: sel.dimId });
    state.dimMemory.set(sel.inst.id + '|' + sel.world.id, sel.dimId);
    cameraFor(sel);
    renderViewer();
    updateCardSelection();
    if (state.detached) syncUrl();
  }

  function coverageArea(rects) {
    return rects.reduce((total, rect) => total + (rect.w * rect.h), 0);
  }

  function coverageCenter(rects) {
    if (!rects.length) return { x: MAP_SIZE / 2, z: MAP_SIZE / 2 };
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const rect of rects) {
      minX = Math.min(minX, rect.x);
      minY = Math.min(minY, rect.y);
      maxX = Math.max(maxX, rect.x + rect.w);
      maxY = Math.max(maxY, rect.y + rect.h);
    }
    return { x: (minX + maxX) / 2, z: (minY + maxY) / 2 };
  }

  function focusPoint(entry) {
    if (entry.last) return { x: entry.last.x, z: entry.last.z };
    return coverageCenter(entry.coverage);
  }

  function outcomeOf(entry) {
    if (!entry?.image || !entry.coverage?.length || imageFor(entry.image).failed) return 'empty';
    return coverageArea(entry.coverage) >= MAP_SIZE * MAP_SIZE ? 'full' : 'partial';
  }

  function nearestZoom(value) {
    let best = ZOOM_LEVELS[0];
    for (const zoom of ZOOM_LEVELS) {
      if (Math.abs(zoom - value) < Math.abs(best - value)) best = zoom;
    }
    return best;
  }

  function initialCamera(entry) {
    const point = focusPoint(entry);
    return { x: point.x, y: point.z, zoom: 1 };
  }

  function cameraKey(sel) {
    return sel.inst.id + '|' + sel.world.id + '|' + sel.dimId;
  }

  function cameraFor(sel) {
    const key = cameraKey(sel);
    let camera = state.cameras.get(key);
    if (!camera) {
      camera = initialCamera(sel.entry);
      state.cameras.set(key, camera);
    }
    return camera;
  }

  function clampCamera(camera) {
    camera.zoom = nearestZoom(Number.isFinite(camera.zoom) ? camera.zoom : 1);
    if (!Number.isFinite(camera.x)) camera.x = MAP_SIZE / 2;
    if (!Number.isFinite(camera.y)) camera.y = MAP_SIZE / 2;
    camera.x = Math.max(0, Math.min(MAP_SIZE, camera.x));
    camera.y = Math.max(0, Math.min(MAP_SIZE, camera.y));
    return camera;
  }

  function afterCameraChange() {
    updateMapChrome(selection());
    scheduleMapDraw();
    syncUrl();
  }

  function zoomBy(step) {
    const sel = selection();
    if (outcomeOf(sel.entry) === 'empty') return;
    const camera = cameraFor(sel);
    const index = ZOOM_LEVELS.indexOf(camera.zoom);
    const next = Math.max(0, Math.min(ZOOM_LEVELS.length - 1, index + step));
    if (next === index) return;
    camera.zoom = ZOOM_LEVELS[next];
    clampCamera(camera);
    afterCameraChange();
  }

  function recenter() {
    const sel = selection();
    if (outcomeOf(sel.entry) === 'empty') return;
    const camera = cameraFor(sel);
    const point = focusPoint(sel.entry);
    camera.x = point.x;
    camera.y = point.z;
    camera.zoom = 1;
    clampCamera(camera);
    afterCameraChange();
  }

  function panBy(dx, dy) {
    const sel = selection();
    if (outcomeOf(sel.entry) === 'empty') return;
    const camera = cameraFor(sel);
    camera.x += dx / camera.zoom;
    camera.y += dy / camera.zoom;
    clampCamera(camera);
    afterCameraChange();
  }

  // ---------------------------------------------------------------------------
  // Library cards
  // ---------------------------------------------------------------------------

  function instanceMeta(instance) {
    if (instance.worlds.length > 1) {
      const servers = instance.worlds.filter((world) => world.kind === 'Saved server').length;
      const local = instance.worlds.length - servers;
      return instance.worlds.length + ' worlds · ' + local + ' local · ' +
        servers + ' saved server' + (servers === 1 ? '' : 's');
    }
    return instance.kind;
  }

  function cardDetail(sel, outcome) {
    const label = sel.entry.label;
    if (outcome === 'empty') return label + ' · no saved terrain';
    if (sel.entry.last) {
      return label + ' · last position ' + sel.entry.last.x + ', ' + sel.entry.last.z;
    }
    if (outcome === 'partial') return label + ' · partial terrain';
    return label + ' · no saved position';
  }

  function buildCards() {
    el.cards.replaceChildren();

    for (const instance of INSTANCES) {
      const featured = resolveSelection(
        instance.id,
        instance.featured.worldId,
        instance.featured.dimId
      );
      const outcome = outcomeOf(featured.entry);

      const card = document.createElement('article');
      card.className = 'card';
      card.dataset.instance = instance.id;
      const glyph = '<img class="glyph" alt="" src="' + icons[instance.id] + '.png">';
      card.innerHTML = [
        '<div class="picture">',
        '  <canvas aria-hidden="true"></canvas>',
        '  <div class="fallback-art" hidden aria-hidden="true">' + glyph + '</div>',
        '  <button class="expand" type="button" aria-haspopup="dialog">⛶</button>',
        '  <div class="tiny-icon" aria-hidden="true">' + glyph + '</div>',
        '</div>',
        '<div class="card-body">',
        '  <div class="card-copy"><h3></h3><p class="meta"></p></div>',
        '  <div class="card-foot"><div class="world"></div><button class="play" type="button"></button></div>',
        '</div>'
      ].join('');

      const canvas = card.querySelector('canvas');
      canvas.dataset.instance = instance.id;
      card.querySelector('h3').textContent = instance.name;
      card.querySelector('.meta').textContent = instanceMeta(instance);

      const worldLine = card.querySelector('.world');
      worldLine.textContent = instance.id === 'redstone-workshop' ? 'No observed world' : featured.world.name;
      const detail = document.createElement('small');
      detail.textContent = cardDetail(featured, outcome);
      worldLine.append(detail);

      const expand = card.querySelector('.expand');
      expand.setAttribute('aria-label', 'Expand ' + instance.name + ' map');
      expand.addEventListener('click', () => {
        openExpanded(expand, featured);
      });

      const play = card.querySelector('.play');
      play.textContent = instance.id === 'north-crossing' ? 'Join server' : 'Play';
      play.addEventListener('click', () => announce('Design preview · game launch unavailable.'));

      card.addEventListener('click', (event) => {
        if (event.target.closest('button')) return;
        const sel = rememberedSelection(instance);
        setSelection(sel.inst.id, sel.world.id, sel.dimId);
        announce(instance.name + ' selected.');
      });

      el.cards.append(card);
    }

    updateCardSelection();
  }

  function updateCardSelection() {
    el.cards.querySelectorAll('.card').forEach((card) => {
      if (card.dataset.instance === state.current.instanceId) {
        card.setAttribute('aria-current', 'true');
      } else {
        card.removeAttribute('aria-current');
      }
    });
  }

  // ---------------------------------------------------------------------------
  // Card previews
  // ---------------------------------------------------------------------------

  function renderPreviews() {
    el.cards.querySelectorAll('.picture canvas').forEach((canvas) => {
      const instance = instanceById.get(canvas.dataset.instance);
      if (instance) drawCardPreview(canvas, instance);
    });
  }

  function drawCardPreview(canvas, instance) {
    const sel = resolveSelection(
      instance.id,
      instance.featured.worldId,
      instance.featured.dimId
    );
    const outcome = outcomeOf(sel.entry);
    const picture = canvas.parentElement;
    const fallback = picture.querySelector('.fallback-art');

    if (outcome === 'empty') {
      canvas.hidden = true;
      if (fallback) fallback.hidden = false;
      return;
    }

    canvas.hidden = false;
    if (fallback) fallback.hidden = true;

    const rect = canvas.getBoundingClientRect();
    const width = Math.round(rect.width);
    const height = Math.round(rect.height);
    if (width < 2 || height < 2) return;

    const dpr = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
    const backingWidth = Math.round(width * dpr);
    const backingHeight = Math.round(height * dpr);
    if (canvas.width !== backingWidth || canvas.height !== backingHeight) {
      canvas.width = backingWidth;
      canvas.height = backingHeight;
    }

    const context = canvas.getContext('2d');
    if (!context) return;
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    context.clearRect(0, 0, width, height);
    context.imageSmoothingEnabled = false;

    const source = sourceFor(sel.entry);
    if (!source) return;

    const focus = focusPoint(sel.entry);
    const scale = Math.max(width / MAP_SIZE, height / MAP_SIZE);
    const originX = width / 2 - focus.x * scale;
    const originY = height / 2 - focus.z * scale;

    context.save();
    context.beginPath();
    for (const rectPart of sel.entry.coverage) {
      context.rect(
        originX + rectPart.x * scale,
        originY + rectPart.y * scale,
        rectPart.w * scale,
        rectPart.h * scale
      );
    }
    context.clip();
    context.drawImage(
      source.element,
      0, 0, MAP_SIZE, MAP_SIZE,
      originX, originY, MAP_SIZE * scale, MAP_SIZE * scale
    );
    context.restore();

    if (sel.entry.last) {
      const markerX = originX + (sel.entry.last.x + 0.5) * scale;
      const markerY = originY + (sel.entry.last.z + 0.5) * scale;
      context.beginPath();
      context.arc(markerX, markerY, 3.5, 0, TAU);
      context.fillStyle = '#1b1c1e';
      context.fill();
      context.lineWidth = 1.5;
      context.strokeStyle = '#f4f3ed';
      context.stroke();
    }
  }

  // ---------------------------------------------------------------------------
  // Viewer chrome and map drawing
  // ---------------------------------------------------------------------------

  function setOptions(select, options, value) {
    const signature = options.map((option) => option.value).join('|');
    if (select.dataset.signature !== signature) {
      select.replaceChildren(...options.map((option) => {
        const node = document.createElement('option');
        node.value = option.value;
        node.textContent = option.label;
        return node;
      }));
      select.dataset.signature = signature;
    }
    select.value = value;
  }

  function renderViewer() {
    const sel = selection();
    el.instanceName.textContent = sel.inst.name + ' · ' + sel.world.kind;
    el.worldTitle.textContent = sel.world.name;

    setOptions(
      el.instanceSelect,
      INSTANCES.map((instance) => ({ value: instance.id, label: instance.name })),
      sel.inst.id
    );
    setOptions(
      el.worldSelect,
      sel.inst.worlds.map((world) => ({ value: world.id, label: world.name })),
      sel.world.id
    );
    setOptions(
      el.dimSelect,
      Object.keys(sel.world.dims).map((id) => ({
        value: id,
        label: sel.world.dims[id].label
      })),
      sel.dimId
    );

    updateMapChrome(sel);
    updateCardSelection();
    scheduleMapDraw();

    if (state.inlineOpen || state.expandedOpen || state.detached) {
      document.title = sel.world.name + ' · Carbon Desktop map';
    }
  }

  function coverageText(entry, outcome) {
    if (outcome === 'empty') return 'Empty · no saved terrain';
    if (outcome === 'partial') {
      const percent = Math.round((coverageArea(entry.coverage) / (MAP_SIZE * MAP_SIZE)) * 100);
      return 'Partial · ' + percent + '% of terrain known';
    }
    return 'Available · full terrain';
  }

  function coordsText(sel, camera) {
    const view = 'View ' + Math.round(camera.x) + ', ' + Math.round(camera.y) +
      ' · ' + camera.zoom + '×';
    if (sel.entry.last) {
      return 'Saved ' + sel.entry.last.x + ', ' + sel.entry.last.z + ' · ' + view;
    }
    return view;
  }

  function mapAria(sel, outcome, camera) {
    if (outcome === 'empty') {
      return 'No saved terrain for ' + sel.world.name + ' in the ' + sel.entry.label + '.';
    }
    const saved = sel.entry.last
      ? ' Saved position X ' + sel.entry.last.x + ', Z ' + sel.entry.last.z + '.'
      : '';
    return sel.world.name + ', ' + sel.entry.label + '. ' + coverageText(sel.entry, outcome) +
      '. View centred at X ' + Math.round(camera.x) + ', Z ' + Math.round(camera.y) +
      ', zoom ' + camera.zoom + '.' + saved +
      ' Arrow keys pan; plus and minus zoom.';
  }

  function updateMapChrome(sel) {
    const outcome = outcomeOf(sel.entry);
    const camera = cameraFor(sel);
    const index = ZOOM_LEVELS.indexOf(camera.zoom);

    el.coverage.textContent = coverageText(sel.entry, outcome);
    el.mapCoords.textContent = coordsText(sel, camera);
    el.emptyMap.hidden = outcome !== 'empty';
    if (outcome === 'empty') {
      el.emptyText.textContent = sel.entry.note || 'This world and dimension have no readable tiles.';
    }

    el.zoomIn.disabled = outcome === 'empty' || index >= ZOOM_LEVELS.length - 1;
    el.zoomOut.disabled = outcome === 'empty' || index <= 0;
    el.center.disabled = outcome === 'empty';
    el.largeMap.setAttribute('aria-disabled', outcome === 'empty' ? 'true' : 'false');
    el.largeMap.setAttribute('aria-label', mapAria(sel, outcome, camera));
  }

  function drawMap() {
    const sel = selection();
    const canvas = el.largeMap;
    const rect = canvas.getBoundingClientRect();
    const width = Math.round(rect.width);
    const height = Math.round(rect.height);
    if (width < 2 || height < 2) return;

    const dpr = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
    const backingWidth = Math.round(width * dpr);
    const backingHeight = Math.round(height * dpr);
    if (canvas.width !== backingWidth || canvas.height !== backingHeight) {
      canvas.width = backingWidth;
      canvas.height = backingHeight;
    }

    const context = canvas.getContext('2d');
    if (!context) return;
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    context.clearRect(0, 0, width, height);
    context.imageSmoothingEnabled = false;

    const outcome = outcomeOf(sel.entry);
    if (outcome === 'empty') {
      updateMapChrome(sel);
      return;
    }

    const source = sourceFor(sel.entry);
    if (!source) {
      updateMapChrome(sel);
      return;
    }

    const camera = cameraFor(sel);
    const zoom = camera.zoom;
    const originX = Math.round(width / 2 - camera.x * zoom);
    const originY = Math.round(height / 2 - camera.y * zoom);
    const viewLeft = (0 - originX) / zoom;
    const viewTop = (0 - originY) / zoom;
    const viewRight = (width - originX) / zoom;
    const viewBottom = (height - originY) / zoom;

    for (const part of sel.entry.coverage) {
      const left = Math.max(part.x, viewLeft);
      const top = Math.max(part.y, viewTop);
      const right = Math.min(part.x + part.w, viewRight);
      const bottom = Math.min(part.y + part.h, viewBottom);
      if (right <= left || bottom <= top) continue;

      const sourceX = Math.max(0, Math.floor(left));
      const sourceY = Math.max(0, Math.floor(top));
      const sourceRight = Math.min(MAP_SIZE, Math.ceil(right));
      const sourceBottom = Math.min(MAP_SIZE, Math.ceil(bottom));
      const sourceWidth = sourceRight - sourceX;
      const sourceHeight = sourceBottom - sourceY;
      if (sourceWidth <= 0 || sourceHeight <= 0) continue;

      context.drawImage(
        source.element,
        sourceX, sourceY, sourceWidth, sourceHeight,
        originX + sourceX * zoom,
        originY + sourceY * zoom,
        sourceWidth * zoom,
        sourceHeight * zoom
      );
    }

    if (sel.entry.last) {
      const markerX = originX + (sel.entry.last.x + 0.5) * zoom;
      const markerY = originY + (sel.entry.last.z + 0.5) * zoom;
      if (markerX > -24 && markerX < width + 24 && markerY > -24 && markerY < height + 24) {
        drawMarker(context, markerX, markerY, zoom);
      }
    }

    updateMapChrome(sel);
  }

  function drawMarker(context, x, y, zoom) {
    const radius = Math.max(5, Math.min(9, zoom + 3));
    context.save();
    context.beginPath();
    context.arc(x, y, radius + 2, 0, TAU);
    context.fillStyle = 'rgba(27, 28, 30, 0.9)';
    context.fill();
    context.beginPath();
    context.arc(x, y, radius, 0, TAU);
    context.lineWidth = 2;
    context.strokeStyle = '#f4f3ed';
    context.stroke();

    context.beginPath();
    context.moveTo(x - radius * 0.5, y);
    context.lineTo(x + radius * 0.5, y);
    context.moveTo(x, y - radius * 0.5);
    context.lineTo(x, y + radius * 0.5);
    context.lineWidth = 1.5;
    context.strokeStyle = '#f4f3ed';
    context.stroke();
    context.restore();
  }

  function scheduleMapDraw() {
    if (state.mapFrame) return;
    state.mapFrame = window.requestAnimationFrame(() => {
      state.mapFrame = 0;
      drawMap();
    });
  }

  function schedulePreviews() {
    if (state.previewFrame) return;
    state.previewFrame = window.requestAnimationFrame(() => {
      state.previewFrame = 0;
      renderPreviews();
    });
  }

  // ---------------------------------------------------------------------------
  // Viewer navigation
  // ---------------------------------------------------------------------------

  function setNav(page) {
    if (page === 'map') {
      el.libraryNav.removeAttribute('aria-current');
      el.mapNav.setAttribute('aria-current', 'page');
    } else {
      el.mapNav.removeAttribute('aria-current');
      el.libraryNav.setAttribute('aria-current', 'page');
    }
  }

  function showInline(sel, focusMap = true) {
    if (state.expandedOpen || state.detached) return;

    setSelection(sel.inst.id, sel.world.id, sel.dimId);
    state.inlineOpen = true;

    if (el.viewer.parentElement !== el.mapTab) el.mapTab.append(el.viewer);
    el.library.hidden = true;
    el.mapTab.hidden = false;
    el.viewer.hidden = false;
    el.dismiss.hidden = true;
    el.expandHere.hidden = false;
    el.detach.hidden = false;
    setNav('map');
    document.title = selection().world.name + ' · Carbon Desktop map';
    scheduleMapDraw();

    if (focusMap) {
      window.requestAnimationFrame(() => {
        el.largeMap.focus({ preventScroll: true });
      });
    }
  }

  function showLibrary() {
    if (state.expandedOpen || state.detached) return;
    el.mapTab.hidden = true;
    el.library.hidden = false;
    el.viewer.hidden = true;
    state.inlineOpen = false;
    setNav('library');
    document.title = DEFAULT_TITLE;
  }

  function showMap(focusMap = true) {
    if (state.expandedOpen || state.detached) return;
    if (!state.inlineOpen) {
      showInline(selection(), focusMap);
      return;
    }
    el.library.hidden = true;
    el.mapTab.hidden = false;
    el.viewer.hidden = false;
    setNav('map');
    document.title = selection().world.name + ' · Carbon Desktop map';
    scheduleMapDraw();
    if (focusMap) {
      window.requestAnimationFrame(() => {
        el.largeMap.focus({ preventScroll: true });
      });
    }
  }

  // ---------------------------------------------------------------------------
  // Expanded map (native fullscreen with overlay fallback)
  // ---------------------------------------------------------------------------

  function openExpanded(invoker, sel) {
    if (state.expandedOpen || state.detached) return;

    setSelection(sel.inst.id, sel.world.id, sel.dimId);
    state.viewerHome = el.viewer.parentElement || el.mapTab;
    state.expandedReturn = invoker;
    state.expandedOpen = true;
    state.savedOverflow = document.body.style.overflow;

    if (el.viewer.parentElement !== el.expanded) el.expanded.append(el.viewer);
    el.expanded.hidden = false;
    el.viewer.hidden = false;
    el.expandHere.hidden = true;
    el.dismiss.hidden = false;
    el.detach.hidden = false;
    el.expanded.setAttribute('role', 'dialog');
    el.expanded.setAttribute('aria-modal', 'true');
    el.expanded.setAttribute('aria-labelledby', 'worldTitle');
    document.body.style.overflow = 'hidden';
    el.shell.setAttribute('inert', '');
    el.review.setAttribute('inert', '');
    document.addEventListener('keydown', trapExpandedKeys, true);
    document.title = selection().world.name + ' · Carbon Desktop map';

    renderViewer();
    el.dismiss.focus({ preventScroll: true });
    requestFullscreen(el.expanded);
    scheduleMapDraw();
  }

  function closeExpanded() {
    if (!state.expandedOpen) return;

    state.expandedOpen = false;
    document.removeEventListener('keydown', trapExpandedKeys, true);
    el.expanded.removeAttribute('aria-modal');
    const leavingFullscreen = isExpandedFullscreen() ? exitFullscreen() : Promise.resolve();

    el.shell.removeAttribute('inert');
    el.review.removeAttribute('inert');
    document.body.style.overflow = state.savedOverflow;

    const home = state.viewerHome && state.viewerHome.isConnected
      ? state.viewerHome
      : el.mapTab;
    home.append(el.viewer);
    state.viewerHome = null;

    el.expanded.hidden = true;
    el.viewer.hidden = !state.inlineOpen;
    el.expandHere.hidden = false;
    el.dismiss.hidden = true;

    const returnTo = state.expandedReturn;
    state.expandedReturn = null;

    document.title = state.inlineOpen
      ? selection().world.name + ' · Carbon Desktop map'
      : DEFAULT_TITLE;

    if (state.inlineOpen) scheduleMapDraw();

    Promise.resolve(leavingFullscreen).finally(() => {
      if (returnTo && returnTo.isConnected && isVisible(returnTo)) {
        returnTo.focus({ preventScroll: true });
      } else if (state.inlineOpen) {
        el.largeMap.focus({ preventScroll: true });
      }
    });
  }

  function isVisible(node) {
    return !!(node && node.getClientRects().length && !node.closest('[hidden]'));
  }

  function isExpandedFullscreen() {
    const fullscreenElement = document.fullscreenElement || document.webkitFullscreenElement;
    return fullscreenElement === el.expanded;
  }

  function requestFullscreen(element) {
    const request = element.requestFullscreen ||
      element.webkitRequestFullscreen ||
      element.msRequestFullscreen;
    if (!request) return;
    try {
      const result = request.call(element);
      if (result && typeof result.catch === 'function') result.catch(() => {});
    } catch (error) {
      // The fixed overlay is the fallback when native fullscreen is unavailable.
    }
  }

  function exitFullscreen() {
    const exit = document.exitFullscreen ||
      document.webkitExitFullscreen ||
      document.msExitFullscreen;
    if (!exit) return;
    try {
      return Promise.resolve(exit.call(document)).catch(() => {});
    } catch (error) {
      // Nothing to do; the overlay is already closing.
    }
  }

  function expandedFocusables() {
    const nodes = el.expanded.querySelectorAll(
      'button:not([disabled]):not([hidden]), select:not([disabled]):not([hidden]), ' +
      'canvas[tabindex]:not([tabindex="-1"]), a[href], [tabindex]:not([tabindex="-1"])'
    );
    return Array.from(nodes).filter((node) => isVisible(node));
  }

  function trapExpandedKeys(event) {
    if (!state.expandedOpen) return;

    if (event.key === 'Escape') {
      event.preventDefault();
      closeExpanded();
      return;
    }

    if (event.key !== 'Tab') return;
    const focusables = expandedFocusables();
    if (!focusables.length) {
      event.preventDefault();
      return;
    }

    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !el.expanded.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !el.expanded.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  }

  function onFullscreenChange() {
    if (state.expandedOpen && !isExpandedFullscreen()) closeExpanded();
  }

  // ---------------------------------------------------------------------------
  // Separate, nonmodal map window
  // ---------------------------------------------------------------------------

  function openSeparateWindow() {
    const sel = selection();
    const camera = cameraFor(sel);
    const url = new URL(window.location.href);
    url.search = '';
    url.hash = '';
    url.searchParams.set('detached', '1');
    url.searchParams.set('instance', sel.inst.id);
    url.searchParams.set('world', sel.world.id);
    url.searchParams.set('dim', sel.dimId);
    url.searchParams.set('x', String(Math.round(camera.x * 100) / 100));
    url.searchParams.set('z', String(Math.round(camera.y * 100) / 100));
    url.searchParams.set('zoom', String(camera.zoom));

    const popup = window.open(
      url.toString(),
      '_blank',
      'width=1180,height=820,resizable=yes,scrollbars=no'
    );

    if (!popup) {
      announce('The separate map window was blocked. Allow pop-ups for this page.');
      return;
    }

    try {
      popup.opener = null;
    } catch (error) {
      // Cross-window access can be limited; the window is already open.
    }
    try {
      popup.focus();
    } catch (error) {
      // Focus is optional.
    }
    announce('Map opened in a separate window.');
  }

  function numberParam(value) {
    if (value === null || value.trim() === '') return null;
    const number = Number(value);
    return Number.isFinite(number) ? number : null;
  }

  function syncUrl() {
    if (!state.detached) return;
    const sel = selection();
    const camera = cameraFor(sel);
    const url = new URL(window.location.href);
    url.searchParams.set('detached', '1');
    url.searchParams.set('instance', sel.inst.id);
    url.searchParams.set('world', sel.world.id);
    url.searchParams.set('dim', sel.dimId);
    url.searchParams.set('x', String(Math.round(camera.x * 100) / 100));
    url.searchParams.set('z', String(Math.round(camera.y * 100) / 100));
    url.searchParams.set('zoom', String(camera.zoom));
    try {
      window.history.replaceState(null, '', url);
    } catch (error) {
      // A file URL may reject history updates; the map itself is unaffected.
    }
  }

  function openDetached(params) {
    const requested = instanceById.get(params.get('instance') || '');
    const sel = resolveSelection(
      requested ? requested.id : INSTANCES[0].id,
      params.get('world'),
      params.get('dim')
    );
    setSelection(sel.inst.id, sel.world.id, sel.dimId);

    const camera = cameraFor(sel);
    const x = numberParam(params.get('x'));
    const z = numberParam(params.get('z'));
    const zoom = numberParam(params.get('zoom'));
    if (x !== null) camera.x = x;
    if (z !== null) camera.y = z;
    if (zoom !== null) camera.zoom = nearestZoom(zoom);
    clampCamera(camera);

    state.detached = true;
    document.body.classList.add('detached');
    el.shell.hidden = true;
    el.review.hidden = true;
    el.expanded.hidden = false;
    el.expanded.setAttribute('aria-label', 'Detached world map');
    el.expanded.append(el.viewer);
    el.viewer.hidden = false;
    el.expandHere.hidden = true;
    el.detach.hidden = true;
    el.dismiss.hidden = false;
    el.dismiss.setAttribute('aria-label', 'Close map window');
    const dismissLabel = el.dismiss.querySelector('span');
    if (dismissLabel) dismissLabel.textContent = 'Close window';

    document.title = sel.world.name + ' · Carbon Desktop map';
    renderViewer();
    scheduleMapDraw();

    window.requestAnimationFrame(() => {
      scheduleMapDraw();
      el.largeMap.focus({ preventScroll: true });
    });
  }

  function closeDetachedWindow() {
    try {
      window.close();
    } catch (error) {
      // Browsers may refuse to close a window they did not open.
    }
    window.setTimeout(() => {
      if (!window.closed) {
        announce('Use the browser window controls to close this map.');
      }
    }, 0);
  }

  // ---------------------------------------------------------------------------
  // Map interactions
  // ---------------------------------------------------------------------------

  function onPointerDown(event) {
    if (event.button !== 0) return;
    const sel = selection();
    if (outcomeOf(sel.entry) === 'empty') return;

    el.largeMap.focus({ preventScroll: true });
    state.drag = {
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY
    };
    try {
      el.largeMap.setPointerCapture(event.pointerId);
    } catch (error) {
      // Pointer capture is a progressive enhancement.
    }
    event.preventDefault();
  }

  function onPointerMove(event) {
    const drag = state.drag;
    if (!drag || drag.pointerId !== event.pointerId) return;

    const sel = selection();
    const camera = cameraFor(sel);
    camera.x -= (event.clientX - drag.x) / camera.zoom;
    camera.y -= (event.clientY - drag.y) / camera.zoom;
    drag.x = event.clientX;
    drag.y = event.clientY;
    clampCamera(camera);
    scheduleMapDraw();
    event.preventDefault();
  }

  function endDrag(event) {
    const drag = state.drag;
    if (!drag || drag.pointerId !== event.pointerId) return;
    state.drag = null;
    try {
      el.largeMap.releasePointerCapture(event.pointerId);
    } catch (error) {
      // Capture may already be gone.
    }
    syncUrl();
  }

  function onMapKeydown(event) {
    const sel = selection();
    if (outcomeOf(sel.entry) === 'empty') return;

    const step = event.shiftKey ? 160 : 40;
    let handled = true;

    switch (event.key) {
      case 'ArrowLeft':
        panBy(-step, 0);
        break;
      case 'ArrowRight':
        panBy(step, 0);
        break;
      case 'ArrowUp':
        panBy(0, -step);
        break;
      case 'ArrowDown':
        panBy(0, step);
        break;
      case '+':
      case '=':
        zoomBy(1);
        break;
      case '-':
      case '_':
        zoomBy(-1);
        break;
      case 'Home':
        recenter();
        break;
      default:
        handled = false;
    }

    if (handled) event.preventDefault();
  }

  // ---------------------------------------------------------------------------
  // Media and synthetic terrain
  // ---------------------------------------------------------------------------

  function imageFor(name) {
    if (!name) return null;
    if (!imageCache.has(name)) {
      const record = { element: new Image(), ok: false };
      record.element.decoding = 'async';
      record.element.onload = () => {
        record.ok = true;
        schedulePreviews();
        scheduleMapDraw();
      };
      record.element.onerror = () => {
        record.failed = true;
        schedulePreviews();
        scheduleMapDraw();
      };
      record.element.src = name;
      imageCache.set(name, record);
    }
    return imageCache.get(name);
  }

  function sourceFor(entry) {
    if (!entry || !entry.coverage || entry.coverage.length === 0) return null;
    const image = entry.image ? imageFor(entry.image) : null;
    if (image && image.ok) {
      return { element: image.element };
    }
    return null;
  }

  // Misc interface
  // ---------------------------------------------------------------------------

  function setVariant(variant) {
    state.variant = variant;
    for (const name of ['scenic', 'split', 'compact']) {
      document.body.classList.toggle(name, name === variant);
    }
    el.variants.querySelectorAll('button').forEach((button) => {
      button.setAttribute('aria-pressed', String(button.dataset.v === variant));
    });
    schedulePreviews();
    scheduleMapDraw();
  }

  function announce(message) {
    el.notice.textContent = message;
    window.clearTimeout(state.noticeTimer);
    state.noticeTimer = window.setTimeout(() => {
      el.notice.textContent = '';
    }, 4000);
  }

  function observeSizes() {
    if ('ResizeObserver' in window) {
      state.previewObserver = new ResizeObserver(schedulePreviews);
      el.cards.querySelectorAll('.picture canvas').forEach((canvas) => {
        state.previewObserver.observe(canvas);
      });
      state.mapObserver = new ResizeObserver(scheduleMapDraw);
      state.mapObserver.observe(el.largeMap);
    } else {
      window.addEventListener('resize', () => {
        schedulePreviews();
        scheduleMapDraw();
      });
    }
  }

  function bindNavigation() {
    el.libraryNav.addEventListener('click', showLibrary);
    el.mapNav.addEventListener('click', () => showMap(true));
  }

  function bindViewer() {
    el.instanceSelect.addEventListener('change', () => {
      const instance = instanceById.get(el.instanceSelect.value) || INSTANCES[0];
      const sel = rememberedSelection(instance);
      setSelection(sel.inst.id, sel.world.id, sel.dimId);
    });

    el.worldSelect.addEventListener('change', () => {
      const current = selection();
      const world = current.inst.worlds.find((candidate) => candidate.id === el.worldSelect.value) ||
        current.inst.worlds[0];
      const remembered = state.dimMemory.get(current.inst.id + '|' + world.id);
      setSelection(current.inst.id, world.id, remembered || world.defaultDim);
    });

    el.dimSelect.addEventListener('change', () => {
      const current = selection();
      setSelection(current.inst.id, current.world.id, el.dimSelect.value);
    });

    el.zoomIn.addEventListener('click', () => zoomBy(1));
    el.zoomOut.addEventListener('click', () => zoomBy(-1));
    el.center.addEventListener('click', recenter);

    el.expandHere.addEventListener('click', () => {
      openExpanded(el.expandHere, selection());
    });

    el.detach.addEventListener('click', openSeparateWindow);

    el.dismiss.addEventListener('click', () => {
      if (state.detached) closeDetachedWindow();
      else closeExpanded();
    });

    el.largeMap.addEventListener('pointerdown', onPointerDown);
    el.largeMap.addEventListener('pointermove', onPointerMove);
    el.largeMap.addEventListener('pointerup', endDrag);
    el.largeMap.addEventListener('pointercancel', endDrag);
    el.largeMap.addEventListener('keydown', onMapKeydown);

    document.addEventListener('fullscreenchange', onFullscreenChange);
    document.addEventListener('webkitfullscreenchange', onFullscreenChange);
  }

  function bindVariants() {
    el.variants.querySelectorAll('button').forEach((button) => {
      button.addEventListener('click', () => setVariant(button.dataset.v));
    });
  }

  function init() {
    buildCards();
    bindNavigation();
    bindViewer();
    bindVariants();
    observeSizes();
    setVariant('scenic');

    const params = new URLSearchParams(window.location.search);
    if (params.get('detached') === '1') {
      openDetached(params);
      return;
    }

    const first = INSTANCES[0];
    setSelection(first.id, first.featured.worldId, first.featured.dimId);
    setNav('library');
    schedulePreviews();
  }

  init();
})();
