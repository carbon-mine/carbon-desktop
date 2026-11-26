(() => {
  "use strict";

  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));
  const clamp = (value, min, max) => Math.min(max, Math.max(min, value));
  const MAP_SRC = "alder-reach.png";
  const MIN_RAIL = 190;
  const MAX_RAIL = 290;
  const DEFAULT_RAIL = 236;

  const ART = {
    fox: { src: "fox.png", bg: "#2a211c", border: "#4a352b", accent: "#d98352" },
    beacon: { src: "beacon.png", bg: "#1c2825", border: "#335047", accent: "#82b7a5" },
    gear: { src: "gear.png", bg: "#292418", border: "#4b4027", accent: "#d5b368" },
    mooncap: { src: "mooncap.png", bg: "#271f2b", border: "#46354e", accent: "#b48bc6" }
  };
  const ART_KEYS = Object.keys(ART);

  const SPECIALS = [
    {
      id: "timberline-trail",
      name: "Timberline Trail",
      mc: "1.21.1",
      loader: "Fabric",
      art: "beacon",
      pinned: true,
      lastPlayed: 12,
      worlds: [
        { name: "Alder Reach", map: true, played: 12, size: "418 MB", focus: "49% 47%" },
        { name: "Ridge Camp", map: false, played: 4380, size: "96 MB", focus: "50% 50%" },
        { name: "Old Survey", map: true, played: 30120, size: "312 MB", focus: "35% 62%" },
        { name: "Kestrel Basin", map: true, played: 100800, size: "201 MB", focus: "68% 34%" },
        { name: "Pinewatch Cut", map: true, played: 187200, size: "274 MB", focus: "58% 28%" }
      ],
      servers: [
        { name: "Harborlight SMP", address: "harborlight.local" }
      ]
    },
    {
      id: "long-way-around",
      name: "The Long Way Around — A Timberline Companion for Extended Seasons",
      mc: "1.21.1",
      loader: "NeoForge",
      art: "fox",
      pinned: true,
      lastPlayed: 340,
      worlds: [
        { name: "Longdraw Survey", map: true, played: 340, size: "1.2 GB", focus: "30% 36%" },
        { name: "Camp Four", map: false, played: 7600, size: "148 MB", focus: "50% 50%" }
      ],
      servers: []
    },
    {
      id: "long-truncation-instance",
      name: "Instance Name Long Enough To Exercise Every Truncation Surface In This Layout",
      mc: "1.20.4",
      loader: "Quilt",
      art: "mooncap",
      pinned: false,
      lastPlayed: null,
      worlds: [],
      servers: []
    },
    {
      id: "cinder-post",
      name: "Cinder Post",
      mc: "1.20.4",
      loader: "Vanilla",
      art: "gear",
      pinned: false,
      lastPlayed: 1540,
      worlds: [
        { name: "Ashfield", map: false, played: 1540, size: "64 MB", focus: "50% 50%" }
      ],
      servers: []
    },
    {
      id: "driftwood-field-notes",
      name: "Driftwood Field Notes",
      mc: "1.21.1",
      loader: "Fabric",
      art: "fox",
      pinned: false,
      lastPlayed: 280,
      worlds: [
        { name: "Driftwood Field", map: true, played: 280, size: "226 MB", focus: "40% 55%" }
      ],
      servers: []
    },
    {
      id: "foxfire-survey",
      name: "Foxfire Survey",
      mc: "1.21.1",
      loader: "Fabric",
      art: "fox",
      pinned: true,
      lastPlayed: 330,
      worlds: [
        { name: "Foxfire Ridge", map: true, played: 330, size: "301 MB", focus: "62% 40%" },
        { name: "Low Heath", map: true, played: 10900, size: "112 MB", focus: "44% 65%" }
      ],
      servers: []
    },
    {
      id: "juniper-vale",
      name: "Juniper Vale",
      mc: "1.20.4",
      loader: "Quilt",
      art: "mooncap",
      pinned: true,
      lastPlayed: 620,
      worlds: [
        { name: "Juniper Vale", map: true, played: 620, size: "287 MB", focus: "46% 54%" },
        { name: "Stone Orchard", map: true, played: 8900, size: "154 MB", focus: "72% 30%" },
        { name: "The Low Terrace", map: false, played: 26000, size: "88 MB", focus: "50% 50%" }
      ],
      servers: []
    },
    {
      id: "harborlight-local",
      name: "Harborlight Local",
      mc: "1.21.1",
      loader: "Vanilla",
      art: "beacon",
      pinned: false,
      lastPlayed: null,
      worlds: [],
      servers: [
        { name: "Harborlight SMP", address: "harborlight.local" }
      ]
    },
    {
      id: "long-compact-pin-camp",
      name: "A Camp With A Name That Is Unreasonably Long For A Compact Pin But Still Fits",
      mc: "1.20.1",
      loader: "Forge",
      art: "gear",
      pinned: false,
      lastPlayed: 84,
      worlds: [
        { name: "Copper Fen", map: true, played: 84, size: "176 MB", focus: "57% 45%" }
      ],
      servers: []
    },
    {
      id: "mosslight-camp",
      name: "Mosslight Camp",
      mc: "1.21.1",
      loader: "Fabric",
      art: "beacon",
      pinned: false,
      lastPlayed: 22,
      worlds: [
        { name: "Mosslight Bank", map: true, played: 22, size: "205 MB", focus: "39% 51%" }
      ],
      servers: []
    }
  ];

  const ROOTS = [
    "Alder", "Bramble", "Cinder", "Copper", "Driftwood", "Ember", "Fern", "Foxfire",
    "Granite", "Juniper", "Kestrel", "Lantern", "Moss", "Nettle", "Oak", "Pine",
    "Quarry", "Rook", "Salt", "Sable", "Thorn", "Umber", "Vesper", "Willow",
    "Yarrow", "Zephyr"
  ];
  const LANDFORMS = [
    "Bend", "Hollow", "Post", "Fen", "Field", "Camp", "Reach", "Vale",
    "Ridge", "Bank", "Loop", "Cove", "Flats", "Point", "Marsh", "Wood"
  ];
  const WORLD_FORMS = [
    "Reach", "Camp", "Basin", "Creek", "Ridge", "Crossing", "Flats", "Hollow",
    "Bank", "Wood", "Vale", "Point", "Marsh", "Fen", "Gulch", "Cove"
  ];
  const MAP_FOCUS = ["48% 46%", "33% 62%", "66% 38%", "52% 55%", "28% 42%", "71% 58%"];
  const PLAY_CYCLE = [42, 135, 275, 540, 860, 1380, 1980, 2880, 4620, 7200, 10080, null, 15360, 21000, 32640];
  const MC_CYCLE = ["1.21.1", "1.21.1", "1.20.4", "1.20.4", "1.20.1", "1.20.1"];
  const LOADER_CYCLE = ["Fabric", "Vanilla", "NeoForge", "Quilt", "Forge"];

  function slug(value) {
    return value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 58);
  }

  function buildInstances() {
    const list = SPECIALS.map((entry) => ({
      ...entry,
      worlds: entry.worlds.map((world) => ({ ...world })),
      servers: entry.servers.map((server) => ({ ...server }))
    }));

    const used = new Set(list.map((entry) => entry.name.toLowerCase()));
    const generatedNames = [];

    for (const root of ROOTS) {
      for (const form of LANDFORMS) {
        const name = `${root} ${form}`;
        const key = name.toLowerCase();
        if (used.has(key)) continue;
        used.add(key);
        generatedNames.push(name);
        if (generatedNames.length >= 90) break;
      }
      if (generatedNames.length >= 90) break;
    }

    generatedNames.forEach((name, index) => {
      const lastPlayed = PLAY_CYCLE[index % PLAY_CYCLE.length];
      const worldCount = 1 + (index % 4);
      const worlds = [];
      const noMapLatest = index % 13 === 5 || index % 29 === 11;
      const root = ROOTS[index % ROOTS.length];

      for (let worldIndex = 0; worldIndex < worldCount; worldIndex += 1) {
        const map = worldIndex === 0 ? !noMapLatest : (index + worldIndex) % 6 !== 0;
        const played = lastPlayed == null ? null : lastPlayed + worldIndex * (1440 + (index % 7) * 360);
        const sizeMb = 64 + ((index * 37 + worldIndex * 19) % 650);
        worlds.push({
          name: `${ROOTS[(index + worldIndex * 3) % ROOTS.length]} ${WORLD_FORMS[(index * 2 + worldIndex) % WORLD_FORMS.length]}`,
          map,
          played,
          size: `${sizeMb} MB`,
          focus: MAP_FOCUS[(index + worldIndex) % MAP_FOCUS.length]
        });
      }

      const servers = [];
      if (index % 17 === 4) {
        servers.push({
          name: `${root} ${["SMP", "Realm", "Server"][index % 3]}`,
          address: `${slug(root)}-${index}.local`
        });
      }

      list.push({
        id: `${slug(name)}-g${index}`,
        name,
        mc: MC_CYCLE[index % MC_CYCLE.length],
        loader: LOADER_CYCLE[index % LOADER_CYCLE.length],
        art: ART_KEYS[(SPECIALS.length + index) % ART_KEYS.length],
        pinned: false,
        lastPlayed,
        worlds,
        servers
      });
    });

    return list;
  }

  const instances = buildInstances();
  const BASE_PINNED = new Set(instances.filter((entry) => entry.pinned).map((entry) => entry.id));

  const frames = {
    a: $('.app-frame[data-variant="a"]'),
    b: $('.app-frame[data-variant="b"]')
  };
  const storyFrames = {
    a: $('.story-frame[data-concept="a"]'),
    b: $('.story-frame[data-concept="b"]')
  };
  const frameViewports = {
    a: $('[data-frame-viewport="a"]'),
    b: $('[data-frame-viewport="b"]')
  };
  const stage = $("#study-stage");

  function baseState(layout = "a", viewport = 1040) {
    return {
      layout,
      viewport,
      active: layout === "b" ? "b" : "a",
      a: {
        selectedId: "timberline-trail",
        query: "",
        railWidth: DEFAULT_RAIL,
        pane: "instances",
        searchReturn: null,
        lastFocus: null,
        modal: null
      },
      b: {
        selectedId: "timberline-trail",
        query: "",
        pane: "home",
        searchReturn: null,
        lastFocus: null,
        modal: null
      }
    };
  }

  function initialState() {
    const params = new URLSearchParams(window.location.search);
    const requestedLayout = params.get("view");
    const layout = requestedLayout === "a" || requestedLayout === "b" || requestedLayout === "both"
      ? requestedLayout
      : "a";
    const viewport = params.get("width") === "735" ? 735 : 1040;
    return baseState(layout, viewport);
  }

  let state = initialState();
  let dragState = null;

  const esc = (value) => String(value).replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;"
  })[character]);

  function ago(minutes) {
    if (minutes == null) return "Never";
    if (minutes < 1) return "Just now";
    if (minutes < 60) return `${Math.round(minutes)}m ago`;
    if (minutes < 1440) return `${Math.round(minutes / 60)}h ago`;
    if (minutes < 2880) return "Yesterday";
    const days = Math.round(minutes / 1440);
    if (days < 14) return `${days}d ago`;
    if (days < 60) return `${Math.round(days / 7)}w ago`;
    return `${Math.round(days / 30)}mo ago`;
  }

  function playedLabel(minutes) {
    return minutes == null ? "Never played" : `Last played ${ago(minutes)}`;
  }

  function getInstance(id) {
    return instances.find((entry) => entry.id === id) || null;
  }

  function selected(variant) {
    return getInstance(state[variant].selectedId) || instances[0];
  }

  function frame(variant) {
    return frames[variant];
  }

  function isFrameVisible(variant) {
    return !storyFrames[variant].hidden;
  }

  function comparePlayed(left, right) {
    const leftValue = left.lastPlayed == null ? Number.POSITIVE_INFINITY : left.lastPlayed;
    const rightValue = right.lastPlayed == null ? Number.POSITIVE_INFINITY : right.lastPlayed;
    if (leftValue !== rightValue) return leftValue - rightValue;
    return left.name.localeCompare(right.name);
  }

  function sortByPlayed(list) {
    return [...list].sort(comparePlayed);
  }

  function filterInstances(query) {
    const needle = query.trim().toLowerCase();
    if (!needle) return sortByPlayed(instances);
    return sortByPlayed(instances.filter((entry) => {
      const haystack = [
        entry.name,
        entry.mc,
        entry.loader,
        ...entry.worlds.map((world) => world.name),
        ...entry.servers.map((server) => server.name)
      ];
      return haystack.some((value) => String(value).toLowerCase().includes(needle));
    }));
  }

  function isVisible(element) {
    return Boolean(
      element &&
      element.isConnected &&
      element.getClientRects().length > 0 &&
      !element.closest("[hidden]")
    );
  }

  function artTile(entry, className = "art-tile") {
    const art = ART[entry.art] || ART.beacon;
    return `<span class="${className}" style="--art-bg:${art.bg};--art-border:${art.border};"><img src="${art.src}" alt="" draggable="false"></span>`;
  }

  function setArt(tile, artKey) {
    if (!tile) return;
    const art = ART[artKey] || ART.beacon;
    tile.style.setProperty("--art-bg", art.bg);
    tile.style.setProperty("--art-border", art.border);
    const image = $("img", tile);
    if (image) image.src = art.src;
  }

  function pinMarkup(pinned) {
    if (!pinned) return `<span class="pin-mark is-empty" aria-hidden="true"></span>`;
    return `<span class="pin-mark" title="Pinned"><svg class="icon" aria-hidden="true"><use href="#i-pin-filled"></use></svg><span class="sr-only">Pinned</span></span>`;
  }

  function railRow(entry) {
    const active = entry.id === state.a.selectedId;
    return `<li>
      <button type="button" class="instance-row${active ? " is-selected" : ""}" data-role="instance-row" data-instance-id="${esc(entry.id)}" ${active ? 'aria-current="true"' : ""} tabindex="-1" title="${esc(entry.name)}">
        ${artTile(entry)}
        <span class="row-text">
          <span class="row-name">${esc(entry.name)}</span>
          <span class="row-meta">${esc(entry.mc)} · ${esc(entry.loader)}</span>
        </span>
      </button>
    </li>`;
  }

  function libraryRow(entry) {
    const active = entry.id === state.b.selectedId;
    return `<li>
      <button type="button" class="library-row${active ? " is-selected" : ""}" data-role="library-row" data-instance-id="${esc(entry.id)}" ${active ? 'aria-current="true"' : ""} tabindex="-1" title="${esc(entry.name)}">
        ${artTile(entry)}
        <span class="row-text">
          <span class="row-name">${esc(entry.name)}</span>
          <span class="row-meta">${esc(entry.loader)} ${esc(entry.mc)} · ${esc(ago(entry.lastPlayed))}</span>
        </span>
        ${pinMarkup(entry.pinned)}
      </button>
    </li>`;
  }

  function emptyState(query, label) {
    return `<li class="empty-state">No ${label} match &ldquo;<strong>${esc(query)}</strong>&rdquo;.<br><button type="button" class="link-button" data-clear-inline>Clear search</button></li>`;
  }

  function syncRoving(list, selectedId) {
    const rows = $$('[data-role="instance-row"], [data-role="library-row"]', list);
    if (!rows.length) return;
    rows.forEach((row) => {
      row.tabIndex = -1;
    });
    const target = rows.find((row) => row.dataset.instanceId === selectedId) || rows[0];
    target.tabIndex = 0;
  }

  function renderRail({ resetScroll = false } = {}) {
    const scroller = $('[data-list-scroll="a"]', frames.a);
    const list = $('[data-list="a"]', frames.a);
    const previousScroll = scroller.scrollTop;
    const query = state.a.query.trim();
    const matches = filterInstances(query);

    if (!matches.length) {
      list.innerHTML = emptyState(query, "instances");
    } else if (query) {
      list.innerHTML = matches.map(railRow).join("");
    } else {
      const pinned = sortByPlayed(matches.filter((entry) => entry.pinned));
      const unpinned = sortByPlayed(matches.filter((entry) => !entry.pinned));
      list.innerHTML = [
        pinned.length ? `<li class="list-group" role="presentation">Pinned</li>${pinned.map(railRow).join("")}` : "",
        `<li class="list-group" role="presentation">Unpinned</li>${unpinned.map(railRow).join("")}`
      ].join("");
    }

    syncRoving(list, state.a.selectedId);
    scroller.scrollTop = resetScroll ? 0 : previousScroll;
    updateCounts();
  }

  function renderLibrary({ resetScroll = false } = {}) {
    const scroller = $('[data-list-scroll="b"]', frames.b);
    const list = $('[data-list="b"]', frames.b);
    const previousScroll = scroller.scrollTop;
    const query = state.b.query.trim();
    const matches = filterInstances(query);

    list.innerHTML = matches.length
      ? matches.map(libraryRow).join("")
      : emptyState(query, "instances");

    syncRoving(list, state.b.selectedId);
    scroller.scrollTop = resetScroll ? 0 : previousScroll;
    updateCounts();
  }

  function updatePinButton(variant) {
    const entry = selected(variant);
    const button = $(`[data-pin="${variant}"]`, frames[variant]);
    if (!button) return;
    button.setAttribute("aria-pressed", String(Boolean(entry.pinned)));
    const label = `${entry.pinned ? "Unpin" : "Pin"} ${entry.name}`;
    button.setAttribute("aria-label", label);
    button.title = label;
    const use = $("use", button);
    if (use) use.setAttribute("href", entry.pinned ? "#i-pin-filled" : "#i-pin");
  }

  function renderDetailA() {
    const root = frames.a;
    const entry = selected("a");
    const latest = entry.worlds[0] || null;
    const hasMap = Boolean(latest && latest.map);

    setArt($('[data-art-tile="a"]', root), entry.art);

    const name = $('[data-instance-name="a"]', root);
    name.textContent = entry.name;
    name.title = entry.name;

    $('[data-instance-meta="a"]', root).textContent = `Minecraft ${entry.mc} · ${entry.loader}${entry.worlds.length ? ` · ${entry.worlds.length} world${entry.worlds.length === 1 ? "" : "s"}` : ""}`;

    updatePinButton("a");

    const mapTitle = $('[data-map-title="a"]', root);
    mapTitle.textContent = latest ? latest.name : "No latest world";
    mapTitle.title = latest ? latest.name : "No latest world";

    const mapOpen = $('[data-open-map="a"]', root);
    mapOpen.hidden = !hasMap;
    if (hasMap) {
      mapOpen.setAttribute("aria-label", `Open map preview for ${latest.name}`);
    }

    const mapImage = $('[data-map-img="a"]', root);
    const mapEmpty = $('[data-map-empty="a"]', root);
    const mapCaption = $('[data-map-caption="a"]', root);

    mapImage.hidden = !hasMap;
    mapEmpty.hidden = hasMap;
    mapCaption.hidden = !hasMap;

    if (hasMap) {
      mapImage.src = MAP_SRC;
      mapImage.alt = `${latest.name} world map`;
      mapImage.style.objectPosition = latest.focus || "50% 50%";
      $('[data-map-recent="a"]', root).textContent = playedLabel(latest.played);
      $('[data-map-size="a"]', root).textContent = latest.size;
    } else {
      const emptyTitle = $('[data-map-empty-title="a"]', root);
      const emptySub = $('[data-map-empty-sub="a"]', root);
      if (latest) {
        emptyTitle.textContent = "No map captured";
        emptySub.textContent = "This world has no captured map yet.";
      } else {
        emptyTitle.textContent = "No local world";
        emptySub.textContent = "Start this instance to create a local world.";
      }
    }

    const worldList = $('[data-worlds="a"]', root);
    if (entry.worlds.length) {
      worldList.innerHTML = entry.worlds.map((world) => {
        const thumbStyle = world.map
          ? `background-image:url('${MAP_SRC}');background-position:${esc(world.focus || "50% 50%")}`
          : "";
        return `<li class="world-row">
          <span class="world-thumb${world.map ? "" : " is-empty"}" style="${thumbStyle}" aria-hidden="true">${world.map ? "" : '<svg class="icon"><use href="#i-folder"></use></svg>'}</span>
          <span class="world-text">
            <span class="world-name" title="${esc(world.name)}">${esc(world.name)}</span>
            <span class="world-meta">${esc(ago(world.played))} · ${esc(world.size)}</span>
          </span>
        </li>`;
      }).join("");
    } else {
      worldList.innerHTML = `<li class="empty-state">No local worlds.</li>`;
    }

    const serverList = $('[data-servers="a"]', root);
    if (entry.servers.length) {
      serverList.innerHTML = entry.servers.map((server, index) => `
        <li class="server-row">
          <span class="server-icon" aria-hidden="true"><svg class="icon"><use href="#i-server"></use></svg></span>
          <span class="server-text">
            <span class="server-name" title="${esc(server.name)}">${esc(server.name)}</span>
            <span class="server-meta">Bound · ${esc(server.address)}</span>
          </span>
          <button type="button" class="join-button" data-action="join" data-server-index="${index}" data-instance-id="${esc(entry.id)}" aria-label="Join ${esc(server.name)}">Join</button>
        </li>
      `).join("");
    } else {
      serverList.innerHTML = `<li class="empty-state">No saved servers.</li>`;
    }

    $('[data-world-count="a"]', root).textContent = String(entry.worlds.length);
    $('[data-server-count="a"]', root).textContent = String(entry.servers.length);
    $('[data-runtime="a"]', root).textContent = `${entry.loader} ${entry.mc}`;
    $('[data-last-played="a"]', root).textContent = ago(entry.lastPlayed);
    updateCounts();
  }

  function renderHeroB() {
    const root = frames.b;
    const entry = selected("b");
    const latest = entry.worlds[0] || null;
    const hasMap = Boolean(latest && latest.map);

    setArt($('[data-art-tile="b"]', root), entry.art);

    const instanceName = $('[data-hero-instance="b"]', root);
    instanceName.textContent = entry.name;
    instanceName.title = entry.name;

    const worldTitle = $('[data-hero-world="b"]', root);
    worldTitle.textContent = latest ? latest.name : "No local world";
    worldTitle.title = latest ? latest.name : "No local world";

    $('[data-hero-meta="b"]', root).textContent = latest
      ? `Minecraft ${entry.mc} · ${entry.loader} · ${playedLabel(latest.played)}`
      : `Minecraft ${entry.mc} · ${entry.loader} · Start a local world to resume.`;

    $('[data-play-label="b"]', root).textContent = latest ? "Resume" : "Play";
    updatePinButton("b");

    const mapImage = $('[data-hero-map-img="b"]', root);
    const mapEmpty = $('[data-hero-map-empty="b"]', root);
    const mapChip = $('[data-open-map="b"]', root);

    mapImage.hidden = !hasMap;
    mapEmpty.hidden = hasMap;
    mapChip.hidden = !hasMap;

    if (hasMap) {
      mapImage.src = MAP_SRC;
      mapImage.alt = `${latest.name} world map`;
      mapImage.style.objectPosition = latest.focus || "50% 50%";
      mapChip.setAttribute("aria-label", `Open map preview for ${latest.name}`);
    } else {
      const emptyTitle = $('[data-hero-map-empty-title="b"]', root);
      const emptySub = $('[data-hero-map-empty-sub="b"]', root);
      if (latest) {
        emptyTitle.textContent = "No map captured";
        emptySub.textContent = "This world is available without a captured map.";
      } else {
        emptyTitle.textContent = "No local world";
        emptySub.textContent = "Start this instance to create one.";
      }
    }
  }

  function renderPinsB() {
    const root = frames.b;
    const list = $('[data-pins="b"]', root);
    const pinned = sortByPlayed(instances.filter((entry) => entry.pinned));
    const count = $('[data-pin-count="b"]', root);
    count.textContent = String(pinned.length);

    if (!pinned.length) {
      list.innerHTML = `<li class="empty-state" style="grid-column:1 / -1">No pinned instances.</li>`;
      return;
    }

    list.innerHTML = pinned.map((entry) => {
      const active = entry.id === state.b.selectedId;
      return `<li>
        <button type="button" class="pin-card${active ? " is-selected" : ""}" data-instance-id="${esc(entry.id)}" ${active ? 'aria-current="true"' : ""} title="${esc(entry.name)}">
          ${artTile(entry)}
          <span class="pin-text">
            <span class="pin-name">${esc(entry.name)}</span>
            <span class="pin-meta">${esc(entry.loader)} ${esc(entry.mc)}</span>
          </span>
        </button>
      </li>`;
    }).join("");
  }

  function updateListSelection(variant) {
    const root = frames[variant];
    const selectedId = state[variant].selectedId;
    const selector = variant === "a" ? '[data-role="instance-row"]' : '[data-role="library-row"]';
    const rows = $$(selector, root);
    rows.forEach((row) => {
      const active = row.dataset.instanceId === selectedId;
      row.classList.toggle("is-selected", active);
      if (active) row.setAttribute("aria-current", "true");
      else row.removeAttribute("aria-current");
      row.tabIndex = -1;
    });
    if (rows.length) {
      const target = rows.find((row) => row.dataset.instanceId === selectedId) || rows[0];
      target.tabIndex = 0;
    }
    if (variant === "b") updatePinSelectionB();
  }

  function updatePinSelectionB() {
    $$(".pin-card", frames.b).forEach((card) => {
      const active = card.dataset.instanceId === state.b.selectedId;
      card.classList.toggle("is-selected", active);
      if (active) card.setAttribute("aria-current", "true");
      else card.removeAttribute("aria-current");
    });
  }

  function updatePaneContext(variant) {
    const context = $(`[data-pane-context="${variant}"]`, frames[variant]);
    if (!context) return;
    const entry = selected(variant);
    context.textContent = entry.name;
    context.title = entry.name;
  }

  function updateCounts() {
    const pinnedCount = instances.filter((entry) => entry.pinned).length;
    const railQuery = state.a.query.trim();
    const railMatches = filterInstances(railQuery).length;
    const libraryQuery = state.b.query.trim();
    const libraryMatches = filterInstances(libraryQuery).length;

    const railCount = $('[data-rail-count="a"]', frames.a);
    railCount.textContent = String(railQuery ? railMatches : instances.length);
    railCount.title = railQuery ? `${railMatches} matching instances` : `${instances.length} local instances`;

    const libraryTotal = $('[data-library-total="b"]', frames.b);
    libraryTotal.textContent = String(libraryQuery ? libraryMatches : instances.length);
    libraryTotal.title = libraryQuery ? `${libraryMatches} matching instances` : `${instances.length} local instances`;
    $('[data-library-count="b"]', frames.b).textContent = libraryQuery
      ? `${libraryMatches} of ${instances.length} instances`
      : `${instances.length} instances`;

    $('[data-pin-count="b"]', frames.b).textContent = String(pinnedCount);
    $('[data-status="a"]', frames.a).textContent = `${instances.length} instances · ${pinnedCount} pinned`;
    $('[data-status="b"]', frames.b).textContent = `${pinnedCount} pinned · ${instances.length} instances`;
  }

  function updateSearchUI(variant) {
    const value = state[variant].query;
    const input = $(`[data-search="${variant}"]`, frames[variant]);
    const wrap = $(`[data-search-wrap="${variant}"]`, frames[variant]);
    const clear = $(`[data-clear="${variant}"]`, frames[variant]);
    const hint = $(`[data-key-hint="${variant}"]`, frames[variant]);

    if (input && input.value !== value) input.value = value;
    if (wrap) wrap.dataset.hasValue = value ? "true" : "false";
    if (clear) clear.hidden = !value;
    if (hint) hint.hidden = Boolean(value);
  }

  function updatePinButtons() {
    updatePinButton("a");
    updatePinButton("b");
  }

  function renderAll() {
    const rail = $('[data-rail="a"]', frames.a);
    rail.style.setProperty("--rail-width", `${state.a.railWidth}px`);
    const resizer = $('[data-resizer="a"]', frames.a);
    resizer.setAttribute("aria-valuenow", String(state.a.railWidth));

    updateSearchUI("a");
    updateSearchUI("b");
    renderRail();
    renderLibrary();
    renderDetailA();
    renderHeroB();
    renderPinsB();
    updatePinButtons();
    updatePaneContext("a");
    updatePaneContext("b");
    updateCounts();
    syncPaneUI("a");
    syncPaneUI("b");
  }

  function selectInstance(variant, id, { origin = "api" } = {}) {
    if (!getInstance(id)) return;
    state[variant].selectedId = id;
    updateListSelection(variant);

    if (variant === "a") {
      renderDetailA();
    } else {
      renderHeroB();
      updatePinSelectionB();
    }

    updatePaneContext(variant);
    setActiveFrame(variant);

    if (variant === "a" && origin === "rail" && state.viewport <= 735) {
      setPane("a", "detail");
      const heading = $('[data-instance-name="a"]', frames.a);
      heading.focus({ preventScroll: true });
    }
  }

  function togglePin(variant, id) {
    const entry = getInstance(id);
    if (!entry) return;
    entry.pinned = !entry.pinned;
    renderRail();
    renderLibrary();
    renderDetailA();
    renderHeroB();
    renderPinsB();
    updatePinButtons();
    updateCounts();
  }

  function setRailWidth(value) {
    const width = clamp(Math.round(Number(value) || DEFAULT_RAIL), MIN_RAIL, MAX_RAIL);
    state.a.railWidth = width;
    const rail = $('[data-rail="a"]', frames.a);
    rail.style.setProperty("--rail-width", `${width}px`);
    const resizer = $('[data-resizer="a"]', frames.a);
    resizer.setAttribute("aria-valuenow", String(width));
  }

  function setPane(variant, pane) {
    const valid = variant === "a" ? ["instances", "detail"] : ["home", "library"];
    if (!valid.includes(pane)) return;
    state[variant].pane = pane;
    syncPaneUI(variant);
  }

  function syncPaneUI(variant) {
    const root = frames[variant];
    root.dataset.pane = state[variant].pane;
    $$(`[data-pane-choice]`, root).forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.paneChoice === state[variant].pane));
    });
  }

  function setActiveFrame(variant) {
    if (!isFrameVisible(variant)) return;
    state.active = variant;
    Object.keys(storyFrames).forEach((key) => {
      storyFrames[key].dataset.active = key === variant ? "true" : "false";
    });
  }

  function resolveActiveVariant() {
    const active = document.activeElement;
    for (const variant of ["a", "b"]) {
      if (isFrameVisible(variant) && frames[variant].contains(active)) return variant;
    }
    if (isFrameVisible(state.active)) return state.active;
    for (const variant of ["a", "b"]) {
      if (isFrameVisible(variant)) return variant;
    }
    return null;
  }

  function resolveModalVariant() {
    const active = document.activeElement;
    for (const variant of ["a", "b"]) {
      if (state[variant].modal && frames[variant].contains(active)) return variant;
    }
    for (const variant of ["a", "b"]) {
      if (state[variant].modal) return variant;
    }
    return null;
  }

  function paneOf(variant, element) {
    if (!element) return null;
    if (variant === "a") {
      if (element.closest(".rail")) return "instances";
      if (element.closest(".detail")) return "detail";
    } else {
      if (element.closest(".library-panel")) return "library";
      if (element.closest(".resume-stack")) return "home";
    }
    return null;
  }

  function handleFocusIn(variant, event) {
    setActiveFrame(variant);
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;

    if (target.closest(".modal-layer")) return;

    if (target.closest(".search-field")) {
      if (!state[variant].searchReturn) {
        const previous = state[variant].lastFocus && state[variant].lastFocus.isConnected
          ? state[variant].lastFocus
          : null;
        state[variant].searchReturn = {
          element: previous,
          pane: paneOf(variant, previous) || state[variant].pane
        };
      }
      return;
    }

    if (target.matches("button, input, a, [role='button'], [tabindex='0']")) {
      state[variant].lastFocus = target;
      const pane = paneOf(variant, target);
      if (pane) state[variant].pane = pane;
    }

    if (!target.closest(".search-field")) {
      state[variant].searchReturn = null;
    }
  }

  function fallbackFocus(variant) {
    const root = frames[variant];
    if (variant === "a") {
      const row = $(".instance-row.is-selected", root) || $(".instance-row", root);
      if (isVisible(row)) return row;
      const play = $('[data-play="a"]', root);
      if (isVisible(play)) return play;
    } else {
      const row = $(".library-row.is-selected", root) || $(".library-row", root);
      if (isVisible(row)) return row;
      const resume = $('[data-play="b"]', root);
      if (isVisible(resume)) return resume;
    }
    return root;
  }

  function focusLibrary(variant) {
    if (!isFrameVisible(variant)) return;
    if (state[variant].modal) return;

    const root = frames[variant];
    const input = $(`[data-search="${variant}"]`, root);
    const active = document.activeElement;
    const activeInFrame = active instanceof HTMLElement && root.contains(active) && !active.closest(".modal-layer");
    let previous = null;

    if (activeInFrame && !active.closest(".search-field")) {
      previous = active;
    } else if (state[variant].lastFocus && state[variant].lastFocus.isConnected && !state[variant].lastFocus.closest("[hidden]")) {
      previous = state[variant].lastFocus;
    } else {
      previous = fallbackFocus(variant);
    }

    const pane = paneOf(variant, previous) || (variant === "a" ? "instances" : "library");
    state[variant].searchReturn = { element: previous, pane };

    if (state.viewport <= 735) {
      setPane(variant, variant === "a" ? "instances" : "library");
    }

    input.focus();
    input.select();
  }

  function restoreSearchFocus(variant) {
    const root = frames[variant];
    const active = document.activeElement;
    if (!(active instanceof HTMLElement) || !active.closest(".search-field") || !root.contains(active)) {
      return false;
    }

    const returnTarget = state[variant].searchReturn;
    if (returnTarget && returnTarget.pane && state.viewport <= 735) {
      setPane(variant, returnTarget.pane);
    }

    let target = returnTarget && isVisible(returnTarget.element) ? returnTarget.element : null;
    if (!target) target = fallbackFocus(variant);
    state[variant].searchReturn = null;
    if (target) target.focus({ preventScroll: false });
    return true;
  }

  function setQuery(variant, value) {
    state[variant].query = value;
    updateSearchUI(variant);
    if (variant === "a") renderRail({ resetScroll: true });
    else renderLibrary({ resetScroll: true });
    updateCounts();
  }

  function clearSearch(variant, { focusInput = true } = {}) {
    state[variant].query = "";
    updateSearchUI(variant);
    if (variant === "a") renderRail({ resetScroll: true });
    else renderLibrary({ resetScroll: true });
    updateCounts();
    if (focusInput) {
      const input = $(`[data-search="${variant}"]`, frames[variant]);
      input.focus();
    }
  }

  function setupListKeyboard(variant) {
    const list = variant === "a" ? $('[data-list="a"]', frames.a) : $('[data-list="b"]', frames.b);

    list.addEventListener("keydown", (event) => {
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      const rows = $$('[data-role="instance-row"], [data-role="library-row"]', list);
      if (!rows.length) return;
      const currentIndex = rows.indexOf(document.activeElement);
      let nextIndex = currentIndex;

      if (event.key === "ArrowDown") nextIndex = currentIndex < 0 ? 0 : Math.min(rows.length - 1, currentIndex + 1);
      if (event.key === "ArrowUp") nextIndex = currentIndex <= 0 ? 0 : currentIndex - 1;
      if (event.key === "Home") nextIndex = 0;
      if (event.key === "End") nextIndex = rows.length - 1;

      event.preventDefault();
      rows[nextIndex].focus();
      rows[nextIndex].scrollIntoView({ block: "nearest" });
    });

    list.addEventListener("focusin", (event) => {
      const row = event.target.closest('[data-role="instance-row"], [data-role="library-row"]');
      if (!row) return;
      $$('[data-role="instance-row"], [data-role="library-row"]', list).forEach((item) => {
        item.tabIndex = item === row ? 0 : -1;
      });
    });
  }

  function trapFocus(event, layer) {
    if (event.key !== "Tab") return;
    const focusable = $$('button:not([disabled]), [href], input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])', layer)
      .filter((element) => isVisible(element));
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;

    if (event.shiftKey && (active === first || !layer.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !layer.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  }

  function closeModal(variant, { restore = true } = {}) {
    const modal = state[variant].modal;
    if (!modal) return;
    const layer = $(`[data-modal="${modal.type}"]`, frames[variant]);
    const trigger = modal.trigger;
    state[variant].modal = null;
    if (layer) layer.hidden = true;
    const shell = $(`[data-shell="${variant}"]`, frames[variant]);
    if (shell) shell.removeAttribute("inert");
    if (restore) {
      let target = isVisible(trigger) ? trigger : null;
      if (!target) target = fallbackFocus(variant);
      if (target) target.focus({ preventScroll: false });
    }
  }

  function openModal(variant, type, trigger) {
    for (const other of ["a", "b"]) {
      if (state[other].modal) closeModal(other, { restore: false });
    }
    const layer = $(`[data-modal="${type}"]`, frames[variant]);
    if (!layer) return;
    const shell = $(`[data-shell="${variant}"]`, frames[variant]);
    state[variant].modal = { type, trigger };
    layer.hidden = false;
    if (shell) shell.setAttribute("inert", "");
    const closeButton = $(".modal-head [data-modal-dismiss]", layer);
    if (closeButton) closeButton.focus({ preventScroll: true });
    setActiveFrame(variant);
  }

  function openMap(variant, trigger) {
    const entry = selected(variant);
    const world = entry.worlds[0];
    if (!world || !world.map) return;
    const layer = $(`[data-modal="map"]`, frames[variant]);
    if (!layer) return;

    const dialogTitle = $('[data-map-dialog-title]', layer);
    dialogTitle.textContent = world.name;
    dialogTitle.title = world.name;
    const dialogSub = $('[data-map-dialog-sub]', layer);
    dialogSub.textContent = `${entry.name} · Minecraft ${entry.mc} · ${entry.loader}`;
    dialogSub.title = `${entry.name} · ${world.name}`;
    const image = $('[data-map-dialog-img]', layer);
    image.src = MAP_SRC;
    image.alt = `${world.name} world map preview`;
    image.style.objectPosition = world.focus || "50% 50%";

    openModal(variant, "map", trigger);
  }

  function openAction(variant, trigger, kind) {
    const entry = selected(variant);
    const layer = $(`[data-modal="action"]`, frames[variant]);
    if (!layer) return;
    const world = entry.worlds[0] || null;
    const targetList = $("[data-target-list]", layer);
    const returnButton = $("[data-action-return]", layer);
    let title = "";
    let rows = [];

    if (kind === "join") {
      const serverIndex = Number(trigger.dataset.serverIndex || 0);
      const server = entry.servers[serverIndex];
      if (!server) return;
      title = "Join requested";
      rows = [
        ["Instance", entry.name],
        ["Server", server.name],
        ["Address", server.address],
        ["Result", "Not connected"]
      ];
      returnButton.textContent = "Return to servers";
    } else {
      const label = kind === "resume" ? "Resume" : "Play";
      title = `${label} requested`;
      rows = [
        ["Instance", entry.name],
        ["World", world ? world.name : "No local world"],
        ["Runtime", `${entry.loader} ${entry.mc}`],
        ["Result", "Not launched"]
      ];
      returnButton.textContent = variant === "b" ? "Return to dashboard" : "Return to instance";
    }

    $("[data-action-title]", layer).textContent = title;
    $("[data-action-sub]", layer).textContent = "No process starts in this layout study.";
    targetList.innerHTML = rows.map(([label, value]) => `<div><dt>${esc(label)}</dt><dd>${esc(value)}</dd></div>`).join("");

    openModal(variant, "action", trigger);
  }

  function setViewport(width) {
    state.viewport = width === 735 ? 735 : 1040;
    $$("[data-width-option]").forEach((button) => {
      button.setAttribute("aria-pressed", String(Number(button.dataset.widthOption) === state.viewport));
    });
    $$(".app-frame").forEach((element) => {
      element.style.width = `${state.viewport}px`;
      element.dataset.viewport = String(state.viewport);
    });
    layoutFrames();
  }

  function setLayout(layout) {
    if (!["a", "b", "both"].includes(layout)) return;
    state.layout = layout;
    storyFrames.a.hidden = layout === "b";
    storyFrames.b.hidden = layout === "a";

    if (layout === "a" && state.b.modal) closeModal("b", { restore: false });
    if (layout === "b" && state.a.modal) closeModal("a", { restore: false });

    if (layout === "a") setActiveFrame("a");
    if (layout === "b") setActiveFrame("b");
    if (layout === "both" && !isFrameVisible(state.active)) setActiveFrame("a");

    $$("[data-layout-option]").forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.layoutOption === layout));
    });

    layoutFrames();
  }

  function layoutFrames() {
    const visible = ["a", "b"].filter(isFrameVisible);
    if (!visible.length) return;

    const width = state.viewport;
    const gap = 24;
    const available = Math.max(320, stage.clientWidth - 48);
    const total = visible.length * width + (visible.length - 1) * gap;
    const scale = Math.max(0.4, Math.min(1, available / total));
    const scaledWidth = Math.round(width * scale);
    const scaledHeight = Math.round(760 * scale);

    visible.forEach((variant) => {
      const shell = storyFrames[variant];
      const viewport = frameViewports[variant];
      const app = frames[variant];
      const size = $(`[data-frame-size="${variant}"]`);
      shell.style.width = `${scaledWidth}px`;
      viewport.style.width = `${scaledWidth}px`;
      viewport.style.height = `${scaledHeight}px`;
      app.style.width = `${width}px`;
      app.style.height = "760px";
      app.dataset.scale = String(scale);
      app.style.transform = scale === 1 ? "" : `scale(${scale})`;
      size.textContent = `${width} × 760${scale < 0.995 ? ` · ${Math.round(scale * 100)}%` : ""}`;
    });
  }

  function resetStudy() {
    for (const variant of ["a", "b"]) {
      if (state[variant].modal) closeModal(variant, { restore: false });
    }
    instances.forEach((entry) => {
      entry.pinned = BASE_PINNED.has(entry.id);
    });

    state = baseState("a", 1040);
    setRailWidth(DEFAULT_RAIL);
    $$("[data-search]").forEach((input) => {
      input.value = "";
    });
    [frames.a, frames.b].forEach((root) => {
      $("[data-list-scroll]", root).scrollTop = 0;
    });
    const detailBody = $(".detail-body", frames.a);
    if (detailBody) detailBody.scrollTop = 0;
    updateSearchUI("a");
    updateSearchUI("b");
    setPane("a", "instances");
    setPane("b", "home");
    setViewport(1040);
    setLayout("a");
    setActiveFrame("a");
    renderAll();
    layoutFrames();
  }

  function installStaticEvents() {
    $$("[data-layout-option]").forEach((button) => {
      button.addEventListener("click", () => setLayout(button.dataset.layoutOption));
    });

    $$("[data-width-option]").forEach((button) => {
      button.addEventListener("click", () => setViewport(Number(button.dataset.widthOption)));
    });

    $("[data-reset]").addEventListener("click", resetStudy);

    ["a", "b"].forEach((variant) => {
      const root = frames[variant];
      root.addEventListener("pointerdown", () => setActiveFrame(variant));
      root.addEventListener("focusin", (event) => handleFocusIn(variant, event));

      $(`[data-pin="${variant}"]`, root).addEventListener("click", () => {
        togglePin(variant, state[variant].selectedId);
      });

      $(`[data-play="${variant}"]`, root).addEventListener("click", (event) => {
        const kind = variant === "b" && selected("b").worlds.length ? "resume" : "play";
        openAction(variant, event.currentTarget, kind);
      });

      $(`[data-open-map="${variant}"]`, root).addEventListener("click", (event) => {
        openMap(variant, event.currentTarget);
      });

      $$(`[data-pane-choice]`, root).forEach((button) => {
        button.addEventListener("click", () => setPane(variant, button.dataset.paneChoice));
      });

      const search = $(`[data-search="${variant}"]`, root);
      search.addEventListener("input", () => setQuery(variant, search.value));
      search.addEventListener("keydown", (event) => {
        if (event.key !== "ArrowDown") return;
        const list = variant === "a" ? $('[data-list="a"]', frames.a) : $('[data-list="b"]', frames.b);
        const row = $('[data-role="instance-row"][tabindex="0"], [data-role="library-row"][tabindex="0"]', list)
          || $('[data-role="instance-row"], [data-role="library-row"]', list);
        if (row) {
          event.preventDefault();
          row.focus();
        }
      });

      $(`[data-clear="${variant}"]`, root).addEventListener("click", () => clearSearch(variant));

      $$(`[data-modal]`, root).forEach((layer) => {
        layer.addEventListener("keydown", (event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            closeModal(variant);
          } else {
            trapFocus(event, layer);
          }
        });
        $$("[data-modal-dismiss], [data-modal-return]", layer).forEach((button) => {
          button.addEventListener("click", () => closeModal(variant));
        });
      });
    });

    $('[data-list="a"]', frames.a).addEventListener("click", (event) => {
      const inlineClear = event.target.closest("[data-clear-inline]");
      if (inlineClear) {
        clearSearch("a");
        return;
      }
      const row = event.target.closest('[data-role="instance-row"]');
      if (!row) return;
      selectInstance("a", row.dataset.instanceId, { origin: "rail" });
    });

    $('[data-list="b"]', frames.b).addEventListener("click", (event) => {
      const inlineClear = event.target.closest("[data-clear-inline]");
      if (inlineClear) {
        clearSearch("b");
        return;
      }
      const row = event.target.closest('[data-role="library-row"]');
      if (!row) return;
      selectInstance("b", row.dataset.instanceId, { origin: "library" });
    });

    $('[data-pins="b"]', frames.b).addEventListener("click", (event) => {
      const card = event.target.closest(".pin-card");
      if (!card) return;
      selectInstance("b", card.dataset.instanceId, { origin: "pin" });
    });

    $('[data-servers="a"]', frames.a).addEventListener("click", (event) => {
      const button = event.target.closest('[data-action="join"]');
      if (!button) return;
      openAction("a", button, "join");
    });

    setupListKeyboard("a");
    setupListKeyboard("b");

    const resizer = $('[data-resizer="a"]', frames.a);
    resizer.addEventListener("pointerdown", (event) => {
      if (state.viewport <= 735) return;
      const rect = frames.a.getBoundingClientRect();
      const scale = frames.a.offsetWidth ? rect.width / frames.a.offsetWidth : 1;
      dragState = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startWidth: state.a.railWidth,
        scale: scale || 1
      };
      resizer.setPointerCapture(event.pointerId);
      resizer.classList.add("is-dragging");
      document.body.classList.add("is-resizing");
      event.preventDefault();
    });

    resizer.addEventListener("pointermove", (event) => {
      if (!dragState || event.pointerId !== dragState.pointerId) return;
      const delta = (event.clientX - dragState.startX) / dragState.scale;
      setRailWidth(dragState.startWidth + delta);
    });

    const endDrag = (event) => {
      if (!dragState || (event && event.pointerId !== dragState.pointerId)) return;
      dragState = null;
      resizer.classList.remove("is-dragging");
      document.body.classList.remove("is-resizing");
      if (event && resizer.hasPointerCapture(event.pointerId)) {
        resizer.releasePointerCapture(event.pointerId);
      }
    };

    resizer.addEventListener("pointerup", endDrag);
    resizer.addEventListener("pointercancel", endDrag);
    resizer.addEventListener("lostpointercapture", () => {
      dragState = null;
      resizer.classList.remove("is-dragging");
      document.body.classList.remove("is-resizing");
    });

    resizer.addEventListener("keydown", (event) => {
      const step = event.shiftKey ? 20 : 8;
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        setRailWidth(state.a.railWidth - step);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        setRailWidth(state.a.railWidth + step);
      } else if (event.key === "Home") {
        event.preventDefault();
        setRailWidth(MIN_RAIL);
      } else if (event.key === "End") {
        event.preventDefault();
        setRailWidth(MAX_RAIL);
      }
    });

    document.addEventListener("keydown", (event) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        const variant = resolveActiveVariant();
        if (variant && !state[variant].modal) focusLibrary(variant);
        return;
      }

      if (event.key === "Escape") {
        const modalVariant = resolveModalVariant();
        if (modalVariant) {
          event.preventDefault();
          closeModal(modalVariant);
          return;
        }
        const variant = resolveActiveVariant();
        if (variant) {
          const active = document.activeElement;
          if (active instanceof HTMLElement && active.closest(".search-field")) {
            event.preventDefault();
            restoreSearchFocus(variant);
          }
        }
      }
    });

    window.addEventListener("resize", layoutFrames);
  }

  function keyDown(element, key, init = {}) {
    element.dispatchEvent(new KeyboardEvent("keydown", {
      key,
      bubbles: true,
      cancelable: true,
      ...init
    }));
  }

  function runSelfTests() {
    const checks = [];
    const check = (name, run) => {
      try {
        const detail = run();
        checks.push({ name, pass: true, detail: detail == null ? "" : String(detail) });
      } catch (error) {
        checks.push({ name, pass: false, detail: error && error.message ? error.message : String(error) });
      }
    };
    const assert = (condition, message) => {
      if (!condition) throw new Error(message || "Assertion failed");
    };

    setLayout("both");
    setViewport(1040);
    clearSearch("a", { focusInput: false });
    clearSearch("b", { focusInput: false });

    check("dataset contains exactly 100 shared instances", () => {
      assert(instances.length === 100, `found ${instances.length}`);
      assert(window.__CARBON_STUDY__ && window.__CARBON_STUDY__.instances === instances, "API is not sharing the dataset");
      return "100";
    });

    check("dataset includes Timberline Trail, long names, an unpinned entry, and no-map cases", () => {
      assert(instances.some((entry) => entry.name === "Timberline Trail"), "Timberline Trail missing");
      const longNames = instances.filter((entry) => entry.name.length > 70).length;
      assert(longNames >= 2, `only ${longNames} long names`);
      assert(instances.some((entry) => !entry.pinned), "no unpinned entry");
      const noMaps = instances.filter((entry) => !entry.worlds[0] || !entry.worlds[0].map).length;
      assert(noMaps >= 2, `only ${noMaps} no-map cases`);
      return `${longNames} long names; ${noMaps} no-map cases`;
    });

    check("both concepts render the full 100-instance library", () => {
      const railRows = $$('[data-role="instance-row"]', frames.a).length;
      const libraryRows = $$('[data-role="library-row"]', frames.b).length;
      assert(railRows === 100, `rail has ${railRows} rows`);
      assert(libraryRows === 100, `library has ${libraryRows} rows`);
      return "100 / 100";
    });

    check("Timberline Trail is selected by default in both concepts", () => {
      assert(state.a.selectedId === "timberline-trail", `A selects ${state.a.selectedId}`);
      assert(state.b.selectedId === "timberline-trail", `B selects ${state.b.selectedId}`);
      return "shared default selection";
    });

    check("rail resize stays inside 190–290 px and preserves selection", () => {
      setViewport(1040);
      selectInstance("a", "driftwood-field-notes");
      const selectedBefore = state.a.selectedId;
      const resizer = $('[data-resizer="a"]', frames.a);
      resizer.focus();
      for (let index = 0; index < 40; index += 1) keyDown(resizer, "ArrowLeft");
      assert(state.a.railWidth === 190, `left bound is ${state.a.railWidth}`);
      assert(state.a.selectedId === selectedBefore, "selection changed while resizing");
      for (let index = 0; index < 60; index += 1) keyDown(resizer, "ArrowRight");
      assert(state.a.railWidth === 290, `right bound is ${state.a.railWidth}`);
      setRailWidth(DEFAULT_RAIL);
      assert(state.a.selectedId === selectedBefore, "selection changed after resize");
      selectInstance("a", "timberline-trail");
      return "190–290 px";
    });

    check("Ctrl+F focuses the full library and Escape restores the prior control", () => {
      setViewport(1040);
      const row = $('.instance-row[data-instance-id="timberline-trail"]', frames.a);
      row.focus();
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true, cancelable: true }));
      const search = $('[data-search="a"]', frames.a);
      assert(document.activeElement === search, "library search did not receive focus");
      assert(state.a.selectedId === "timberline-trail", "selection changed when focusing library");
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      assert(document.activeElement === row, "focus was not restored to the row");
      return "focus returned";
    });

    check("search filters the library without changing selection", () => {
      const search = $('[data-search="a"]', frames.a);
      search.value = "timberline";
      search.dispatchEvent(new Event("input", { bubbles: true }));
      const matches = $$('[data-role="instance-row"]', frames.a).length;
      assert(matches >= 2, `found ${matches} matches`);
      assert($('[data-rail-count="a"]', frames.a).textContent === String(matches), "result count is wrong");
      assert(state.a.selectedId === "timberline-trail", "filtering changed selection");
      clearSearch("a", { focusInput: false });
      return `${matches} matches`;
    });

    check("map preview opens, Escape returns exact focus, and the return button does too", () => {
      selectInstance("a", "timberline-trail");
      const trigger = $('[data-open-map="a"]', frames.a);
      assert(!trigger.hidden, "map trigger is hidden for a mapped world");
      trigger.focus();
      trigger.click();
      const layer = $('[data-modal="map"]', frames.a);
      assert(!layer.hidden, "map preview did not open");
      assert($('[data-map-dialog-title]', layer).textContent === "Alder Reach", "map preview target is wrong");
      layer.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      assert(layer.hidden, "Escape did not close the map preview");
      assert(document.activeElement === trigger, "Escape did not restore exact map focus");
      trigger.focus();
      trigger.click();
      $("[data-modal-return]", layer).click();
      assert(layer.hidden, "return button did not close the map preview");
      assert(document.activeElement === trigger, "return button did not restore exact map focus");
      return "focus restored";
    });

    check("Resume opens a simulator-only result identifying its target", () => {
      selectInstance("b", "timberline-trail");
      const trigger = $('[data-play="b"]', frames.b);
      trigger.focus();
      trigger.click();
      const layer = $('[data-modal="action"]', frames.b);
      assert(!layer.hidden, "Resume result did not open");
      const text = $("[data-target-list]", layer).textContent;
      assert(text.includes("Timberline Trail"), "target instance missing");
      assert(text.includes("Alder Reach"), "target world missing");
      assert(text.includes("Not launched"), "launch state missing");
      $("[data-action-return]", layer).click();
      assert(layer.hidden, "return did not close Resume result");
      assert(document.activeElement === trigger, "Resume focus was not restored");
      return "target identified";
    });

    check("a bound server exposes exactly one Join action", () => {
      selectInstance("a", "timberline-trail");
      const joins = $$('[data-action="join"]', frames.a);
      assert(joins.length === 1, `found ${joins.length} Join controls`);
      assert(joins[0].closest(".server-row").textContent.includes("Harborlight SMP"), "wrong bound server");
      return "single Join";
    });

    check("pin changes propagate through the shared dataset without moving selection", () => {
      selectInstance("a", "driftwood-field-notes");
      selectInstance("b", "driftwood-field-notes");
      const beforeA = state.a.selectedId;
      const beforeB = state.b.selectedId;
      const pinButton = $('[data-pin="a"]', frames.a);
      pinButton.click();
      assert(getInstance("driftwood-field-notes").pinned, "pin did not update the dataset");
      assert($('[data-pin-count="b"]', frames.b).textContent === "5", "B pin count did not update");
      assert($$('.pin-card', frames.b).some((card) => card.dataset.instanceId === "driftwood-field-notes"), "B pin list did not update");
      assert(state.a.selectedId === beforeA && state.b.selectedId === beforeB, "selection moved when pinning");
      pinButton.click();
      assert(!getInstance("driftwood-field-notes").pinned, "unpin did not update the dataset");
      assert($('[data-pin-count="b"]', frames.b).textContent === "4", "B pin count did not restore");
      selectInstance("a", "timberline-trail");
      selectInstance("b", "timberline-trail");
      return "shared pin state";
    });

    check("no-map selection shows a complete empty state and hides the preview trigger", () => {
      selectInstance("a", "cinder-post");
      assert(!$('[data-map-empty="a"]', frames.a).hidden, "A map empty state is hidden");
      assert($('[data-open-map="a"]', frames.a).hidden, "A map trigger is visible without a map");
      selectInstance("b", "cinder-post");
      assert(!$('[data-hero-map-empty="b"]', frames.b).hidden, "B map empty state is hidden");
      assert($('[data-open-map="b"]', frames.b).hidden, "B map trigger is visible without a map");
      selectInstance("a", "timberline-trail");
      selectInstance("b", "timberline-trail");
      return "both concepts";
    });

    check("long names remain complete in the DOM and truncate visually by title", () => {
      const entry = instances.find((item) => item.name.startsWith("Instance Name Long Enough"));
      assert(entry, "long-name entry missing");
      selectInstance("a", entry.id);
      const heading = $('[data-instance-name="a"]', frames.a);
      assert(heading.textContent === entry.name, "heading lost the full name");
      assert(heading.title === entry.name, "heading title is incomplete");
      selectInstance("a", "timberline-trail");
      return `${entry.name.length} characters`;
    });

    check("735 px uses a switchable single pane and keeps selection", () => {
      setViewport(735);
      setPane("a", "instances");
      assert(getComputedStyle($('[data-pane-switch="a"]', frames.a)).display === "flex", "A pane switch is hidden");
      assert(getComputedStyle($(".rail", frames.a)).display !== "none", "A rail is hidden in list pane");
      assert(getComputedStyle($(".detail", frames.a)).display === "none", "A detail is visible in list pane");
      $('[data-pane-choice="detail"]', frames.a).click();
      assert(getComputedStyle($(".detail", frames.a)).display !== "none", "A detail did not show");
      assert(getComputedStyle($(".rail", frames.a)).display === "none", "A rail did not hide");

      setPane("b", "home");
      assert(getComputedStyle($(".resume-stack", frames.b)).display !== "none", "B resume stack did not show");
      assert(getComputedStyle($(".library-panel", frames.b)).display === "none", "B library is visible in home pane");
      $('[data-pane-choice="library"]', frames.b).click();
      assert(getComputedStyle($(".library-panel", frames.b)).display !== "none", "B library did not show");
      assert(getComputedStyle($(".resume-stack", frames.b)).display === "none", "B resume stack did not hide");

      setViewport(1040);
      assert(state.a.selectedId === "timberline-trail", "selection changed across viewport resize");
      return "single pane at 735";
    });

    check("Ctrl+F crosses panes at 735 px and Escape returns to the exact detail control", () => {
      setViewport(735);
      setPane("a", "detail");
      const trigger = $('[data-play="a"]', frames.a);
      trigger.focus();
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true, cancelable: true }));
      assert(frames.a.dataset.pane === "instances", "Ctrl+F did not reveal the library pane");
      assert(document.activeElement === $('[data-search="a"]', frames.a), "search did not receive focus");
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      assert(frames.a.dataset.pane === "detail", "Escape did not restore the detail pane");
      assert(document.activeElement === trigger, "Escape did not return to the Play control");
      setViewport(1040);
      return "pane + focus restored";
    });

    check("selection survives navigation between library and detail", () => {
      selectInstance("a", "driftwood-field-notes");
      const before = state.a.selectedId;
      setViewport(735);
      setPane("a", "instances");
      setPane("a", "detail");
      setViewport(1040);
      assert(state.a.selectedId === before, "A selection changed");
      const row = $(`.instance-row[data-instance-id="${before}"]`, frames.a);
      assert(row && row.classList.contains("is-selected"), "A selected row lost its state");
      selectInstance("a", "timberline-trail");
      return before;
    });

    check("the document has no external script or stylesheet dependency", () => {
      const external = $$('script[src], link[rel="stylesheet"][href]').filter((element) => {
        const url = element.src || element.href || "";
        return /^https?:/i.test(url);
      });
      assert(external.length === 0, `${external.length} external references found`);
      return "local files only";
    });

    resetStudy();

    const passed = checks.filter((item) => item.pass).length;
    const report = {
      status: passed === checks.length ? "pass" : "fail",
      passed,
      total: checks.length,
      viewport: "1040 × 760 / 735 × 760",
      checks
    };

    document.documentElement.dataset.selftest = report.status;
    const pre = document.createElement("pre");
    pre.id = "self-test-report";
    pre.hidden = true;
    pre.textContent = JSON.stringify(report, null, 2);
    document.body.appendChild(pre);
  }

  function init() {
    installStaticEvents();
    renderAll();
    setViewport(state.viewport);
    setLayout(state.layout);
    setActiveFrame(state.layout === "b" ? "b" : state.layout === "a" ? "a" : state.active);
    setPane("a", state.a.pane);
    setPane("b", state.b.pane);
    layoutFrames();

    window.addEventListener("load", layoutFrames);
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(layoutFrames).catch(() => {});
    }

    window.__CARBON_STUDY__ = {
      instances,
      get state() { return state; },
      setViewport,
      setLayout,
      selectInstance,
      setPane,
      setRailWidth,
      resetStudy,
      runSelfTests
    };

    if (new URLSearchParams(window.location.search).get("selftest") === "1") {
      runSelfTests();
    }
  }

  init();
})();
