/* SPDX-License-Identifier: MPL-2.0 */

const byId = (id) => document.getElementById(id);
const preview = byId('preview');
const modal = byId('review');
const systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
const samples = [
    ['meadow', 'Meadow Survival', '1.21.11 · Vanilla', 'ready', '#42be65'],
    ['stone', 'Stone &amp; Steam', '1.21.1 · NeoForge', 'update', '#ffb784'],
    ['redstone', 'Redstone Lab', '1.21.11 · Vanilla', 'running', '#a6c8ff'],
    ['map', 'Map Workshop', '1.20.1 · Fabric', 'interrupted', '#be95ff'],
    ['snapshot', 'Snapshot Test Bench', 'Snapshot · Vanilla', 'uninstalled', '#3ddbd9'],
    ['legacy', 'Legacy 1.12.2', '1.12.2 · Forge', 'ready', '#ff8389']
];
const labels = {
    ready: 'Ready', update: 'Update available', running: 'Running',
    interrupted: 'Interrupted', uninstalled: 'Install needed', installing: 'Installing'
};
const directions = {
    a: [
        'A · Split library. Compare several instances while the selected instance and task strip stay in view.',
        'Wide: choose an instance from the left pane. The right pane displays its state and one primary action.',
        'Narrow: open a single detail view. Back restores the Library selection and query. Activity remains below either view.',
        'Review Stone &amp; Steam’s update, keep the current version, then choose another instance. The Map Workshop interruption remains visible.'
    ],
    b: [
        'B · Library grid. Scan the catalogue and open a detail page to act. Interruptions belong to the relevant tile and detail page.',
        'The Library owns New, Import and Search. Open Meadow Survival → Play. The action names its instance in context.',
        'Back restores focus and search. The catalogue becomes one column when space runs out.',
        'Open Map Workshop to recover the download. A failure outside the current view is easier to miss.'
    ],
    c: [
        'C · Play home. Start the recent instance from Home, browse a dedicated Library, and open Activity from any view.',
        'Play is visible on Home for a ready recent instance. Advanced actions stay under the instance in Library.',
        'Open Activity → Map Workshop → Resume install. Close the progress dialog and continue browsing.',
        'Home adds a step for players who mainly switch packs. Compare this direction with A using the 72-instance fixture.'
    ]
};
let items, selected, recent, route, query, filter, sort, fromTasks, opener, searchTimer;
const direction = () => byId('direction').value;
const active = () => items.filter((item) => ['installing', 'interrupted'].includes(item.state));
const itemFrom = (id) => items.find((item) => item.id === id);
const button = (label, action, cls = '', target = '') =>
    `<button type="button" data-action="${action}" data-target="${target}" class="${cls}">${label}</button>`;
const tag = (item) => `<span class="tag" data-state="${item.state}">${labels[item.state]}</span>`;
const makeItem = ([id, name, info, state, color]) => ({ id, name, info, state, color });

function reset() {
    const librarySize = byId('size').value;
    items = (librarySize === 'empty' ? [] : librarySize === 'single' ? samples.slice(0, 1) : samples).map(makeItem);
    if (librarySize === 'large') {
        const groups = ['Workshop', 'World', 'Build', 'Lab', 'Archive', 'Survival'];
        for (let index = 0; index < 66; index++) {
            const group = index % groups.length;
            items.push(makeItem(['fixture-' + index, groups[group] + ' ' + (index + 7), samples[group][2],
                index % 3 ? 'ready' : 'uninstalled', samples[group][4]]));
        }
    }
    selected = recent = items[0]?.id;
    route = direction() === 'c' ? 'home' : 'library';
    query = '';
    filter = 'all';
    sort = 'recent';
    fromTasks = false;
    ['summary', 'notes'].forEach((id) => {
        const [summary, ...notes] = directions[direction()];
        if (id === 'summary') byId(id).textContent = summary;
        else byId(id).innerHTML = notes.map((note) => '<li>' + note + '</li>').join('');
    });
    refresh();
}

function theme() {
    const preference = byId('theme').value;
    const current = preference === 'system' ? (systemTheme.matches ? 'dark' : 'light') : preference;
    document.documentElement.dataset.carbonTheme = current;
    byId('brand-mark').src = '../../design-system/inherited/brand/mark-' + (current === 'dark' ? 'light' : 'dark') + '.svg';
}

function starter() {
    return `<section class="empty"><h2>Set up your first instance</h2>
        <p>Create a separate Minecraft Java setup for your worlds and settings, or bring in a supported pack file.</p>
        <div class="commands">${button('New vanilla instance', 'new', 'primary')} ${button('Import a file', 'import')}</div>
    </section>`;
}

function collection(grid) {
    return `<section class="${grid ? 'collection' : ''}" aria-label="Browse instances">
        <div class="library-head"><h2 id="library-heading" tabindex="-1">Library</h2>
            <div class="commands">${button('New', 'new')} ${button('Import', 'import')}</div>
        </div>
        <div class="commands">
            <label class="field${grid ? '' : ' search'}">Find an instance<input type="search" id="query" autocomplete="off"></label>
            <label class="field">Status<select id="filter"><option value="all">All</option><option value="ready">Ready</option><option value="update">Updates</option><option value="interrupted">Interrupted</option></select></label>
            <label class="field">Sort<select id="sort"><option value="recent">Recent</option><option value="name">Name</option></select></label>
        </div>
        <p class="meta" id="result-count"></p><div id="results"></div>
    </section>`;
}

function renderResults() {
    const destination = byId('results');
    if (!destination) return;
    const found = items.filter((item) =>
        (filter === 'all' || filter === item.state) &&
        [item.name, item.info, labels[item.state]].some((value) => value.toLowerCase().includes(query.toLowerCase().trim())));
    if (sort === 'name') found.sort((first, second) => first.name.localeCompare(second.name));
    const count = found.length;
    byId('result-count').textContent = count + (count === 1 ? ' instance' : ' instances');
    const oldScroll = destination.firstElementChild?.scrollTop;
    destination.innerHTML = count ? `<ul class="${direction() === 'a' ? 'list' : 'grid'}">${found.map((item) => `
        <li><button type="button" class="instance" data-action="details" data-target="${item.id}"
            ${direction() === 'a' && item.id === selected ? 'aria-current="true"' : ''}>
            <span class="instance-mark" style="--chip:${item.color}" aria-hidden="true"></span>
            <span><strong>${item.name}</strong><span class="meta">${item.info} · ${labels[item.state]}</span></span>
        </button></li>`).join('')}</ul>` : '<p class="block">No instances match. Try a different search or status filter.</p>';
    if (oldScroll) destination.firstElementChild.scrollTop = oldScroll;
}

function primary(item) {
    const actions = {
        ready: ['Play', 'play'], update: ['Review update', 'review'], interrupted: ['Resume install', 'resume'],
        uninstalled: ['Install', 'install'], installing: ['View progress', 'job']
    };
    const choice = actions[item.state];
    return choice ? button(...choice, 'primary', item.id) : '<span class="meta">Game window open</span>';
}

function detail(item) {
    const message = {
        interrupted: '<div class="block alert-mark"><h3>Download interrupted</h3><p>Connection lost. Resume to check staged files and fetch what is missing.</p></div>',
        update: `<div class="block"><h3>Update available</h3><p>Review the proposed changes or play the current installation.</p><div class="commands">${button('Play current', 'play', '', item.id)}</div></div>`,
        installing: '<div class="block"><h3>Downloading game files</h3><p>Verification comes next. You can use other instances while this runs.</p></div>'
    };
    return `<section class="pane" data-mobile-show="${route === 'detail'}">
        <div class="commands ${direction() === 'a' ? 'mobile-only' : ''}">${button(fromTasks ? 'Back to Activity' : 'Back to Library', 'back')}</div>
        <div class="details-head">
            <div><p class="eyebrow">${item.info}</p><h2 id="instance-heading" tabindex="-1">${item.name}</h2></div>
            <div class="commands">${tag(item)} ${primary(item)}</div>
        </div>
        ${message[item.state] || ''}
        <div class="block">
            <div class="section-title"><h3>Instance tools</h3>
                <button type="button" data-action="commands" aria-expanded="false" aria-controls="instance-tools">Commands</button>
            </div>
            <p class="meta">Manage this instance’s Java, files and diagnostics.</p>
            <div class="commands" id="instance-tools" hidden>${['Java settings', 'Clone', 'Repair', 'View log'].map((label) => button(label, 'placeholder', '', label)).join(' ')}</div>
        </div>
    </section>`;
}

function home() {
    const item = itemFrom(recent);
    if (!item) return starter();
    return `<section class="collection">
        <section class="hero"><p class="eyebrow">Last played</p>
            <h2 id="home-heading" tabindex="-1">${item.name}</h2><p class="meta">${item.info}</p>
            <div class="commands">${tag(item)} ${primary(item)} ${button('Open instance', 'details', '', item.id)}</div>
        </section>
        <div class="block row-between"><p><strong>Your library</strong><br><span class="meta">${items.length} ${items.length === 1 ? 'instance' : 'instances'}</span></p>
            ${button('Browse library', 'library')}
        </div>
    </section>`;
}

function tasks() {
    const jobs = active();
    const content = jobs.map((item) => {
        const broken = item.state === 'interrupted';
        return `<div class="block ${broken ? 'alert-mark' : ''}">
            <h3>${item.name}</h3><p>${broken ? 'Download interrupted. The existing instance is intact.' : 'Downloading game files.'}</p>
            <div class="commands">${button('Open instance', 'details', '', item.id)} ${button(broken ? 'Resume install' : 'View progress', broken ? 'resume' : 'job', '', item.id)}</div>
        </div>`;
    }).join('');
    return `<section class="collection"><h2 id="activity-heading" tabindex="-1">Activity</h2>${content || '<p class="block">No active installations or interruptions.</p>'}</section>`;
}

function strip() {
    const jobs = active();
    const download = jobs.find((item) => item.state === 'installing');
    const interrupted = jobs.find((item) => item.state === 'interrupted');
    if (!jobs.length) return '<div class="activity"><span class="meta">No active installations</span></div>';
    return `<section class="activity" aria-label="Installation activity"><div class="activity-text">
        ${download ? `<div class="commands"><strong>Installing ${download.name}</strong>
        <progress value="42" max="100" aria-label="${download.name} fixture progress"></progress><span class="tech">42%</span></div>` : ''}
        ${interrupted ? `<p><strong>Interrupted</strong> · ${interrupted.name}</p>` : ''}
        </div>${button('View activity (' + jobs.length + ')', 'activity')}</section>`;
}

function refresh() {
    preview.dataset.width = byId('width').value;
    const previousScroll = preview.querySelector('.list')?.scrollTop;
    const header = `<header class="app-head">
        <span class="brand"><img id="brand-mark" src="../../design-system/inherited/brand/mark-light.svg" alt="">Carbon Desktop</span>
        <nav class="commands" aria-label="Launcher navigation">
            ${direction() === 'c' ? button('Home', 'home') : ''}
            ${direction() === 'c' || route === 'activity' ? button('Library', 'library') : ''}
            ${direction() === 'c' ? button('Activity (' + active().length + ')', 'activity') : ''}
            ${button('Account', 'account')}
        </nav>
    </header>`;
    let body;
    if (route === 'activity') body = tasks();
    else if (direction() === 'c' && route === 'home') body = home();
    else if (!items.length) body = starter();
    else if (direction() === 'a') body = `<div class="split"><aside data-mobile-show="${route !== 'detail'}">${collection(false)}</aside>${detail(itemFrom(selected))}</div>`;
    else if (route === 'detail') body = detail(itemFrom(selected));
    else body = collection(true);
    preview.innerHTML = header + '<div class="workspace">' + body + '</div>' + (direction() === 'a' && route !== 'activity' ? strip() : '');
    if (byId('query')) {
        byId('query').value = query;
        byId('filter').value = filter;
        byId('sort').value = sort;
    }
    renderResults();
    if (previousScroll && preview.querySelector('.list')) preview.querySelector('.list').scrollTop = previousScroll;
    theme();
}

function focusAction(action, id) {
    const controls = [...preview.querySelectorAll('[data-action]')];
    const match = controls.find((control) => control.dataset.action === action &&
        (!id || id === control.dataset.target) && control.getClientRects().length);
    const alternate = ['instance-heading', 'activity-heading', 'home-heading', 'library-heading']
        .map(byId).find((element) => element?.getClientRects().length);
    (match || alternate || preview.querySelector('button'))?.focus();
}

function speak(text) {
    byId('announce').textContent = text;
}

function open(title, content) {
    opener = document.activeElement;
    byId('dialog-content').innerHTML = `<h2 id="dialog-title">${title}</h2>${content}`;
    modal.showModal();
    modal.querySelector('button').focus();
}

function changeState(item, state, action, message) {
    if (modal.open) modal.close();
    item.state = state;
    refresh();
    focusAction(action, item.id);
    speak(message);
}

function click(event) {
    const control = event.target.closest('button[data-action]');
    if (!control) return;
    const { action, target } = control.dataset;
    const item = itemFrom(target);
    switch (action) {
        case 'details':
            fromTasks = route === 'activity';
            selected = target;
            route = 'detail';
            refresh();
            byId('instance-heading')?.focus();
            break;
        case 'home':
        case 'library':
        case 'activity':
            route = action;
            refresh();
            (byId(action + '-heading') || preview.querySelector('.empty button'))?.focus();
            break;
        case 'back':
            route = fromTasks ? 'activity' : 'library';
            fromTasks = false;
            refresh();
            focusAction('details', selected);
            break;
        case 'commands':
            byId('instance-tools').hidden = !byId('instance-tools').hidden;
            control.setAttribute('aria-expanded', String(!byId('instance-tools').hidden));
            break;
        case 'close':
            modal.close();
            break;
        case 'play':
            recent = item.id;
            changeState(item, 'running', 'commands', 'The game is running in the fixture.');
            break;
        case 'review':
            open('Review update', `<p><strong>${item.name}</strong> · release A → B</p><ul><li>Update 3 mods.</li><li>Reuse 14 verified files.</li><li>Keep the current worlds and personal settings.</li></ul><p>Fixture release review. Activation waits until the game exits.</p><div class="commands">${button('Keep current', 'close')} ${button('Stage update', 'install', 'primary', item.id)}</div>`);
            break;
        case 'install':
        case 'resume':
            changeState(item, 'installing', 'job', 'Installing game files. Verification comes after download.');
            break;
        case 'job':
            open('Installation details', `<p><strong>${item.name}</strong></p><p>Downloading · 42% (fixture).</p><p>Leaving this view keeps the download going.</p><div class="commands">${button('Keep installing', 'close')} ${button('Cancel download', 'cancel-job', '', item.id)} ${button('Simulate interruption', 'interrupt', '', item.id)} ${button('Simulate completion', 'complete', 'primary', item.id)}</div>`);
            break;
        case 'cancel-job':
            changeState(item, item.id === 'stone' ? 'update' : 'uninstalled', item.id === 'stone' ? 'review' : 'install', 'Installation cancelled.');
            break;
        case 'interrupt':
            changeState(item, 'interrupted', 'resume', 'Download interrupted. Resume available.');
            break;
        case 'complete':
            changeState(item, 'ready', 'play', 'Installation verified. Ready to play.');
            break;
        case 'account':
            open('Minecraft account', `<p>Sign-in and entitlement checks use the approved browser flow. This wireframe uses sample account state.</p><div class="commands">${button('Close', 'close')}</div>`);
            break;
        case 'new':
        case 'import':
            open(action === 'new' ? 'New vanilla instance' : 'Import a file', action === 'new'
                ? `<p>Use the sample Minecraft 1.21.11 release. Create the profile before starting a download.</p><div class="commands">${button('Cancel', 'close')} ${button('Create sample instance', 'add-example', 'primary', 'new')}</div>`
                : `<p>Choose an archive in the native file picker. The app checks entries and source availability before showing what will be imported.</p><div class="commands">${button('Cancel', 'close')} ${button('Review sample archive', 'import-review', 'primary')}</div>`);
            break;
        case 'import-review':
            byId('dialog-content').innerHTML = `<h2 id="dialog-title">Review import</h2><p>sample-pack.mrpack · 1.21.11 · Fabric</p><p>Two mods and one config. The sample contains no worlds.</p><div class="commands">${button('Cancel', 'close')} ${button('Create imported instance', 'add-example', 'primary', 'import')}</div>`;
            modal.querySelector('button').focus();
            break;
        case 'add-example': {
            modal.close();
            selected = 'new-' + (items.length + 1);
            items.unshift(makeItem([selected, target === 'new' ? 'New Vanilla' : 'Imported Pack',
                target === 'new' ? '1.21.11 · Vanilla' : '1.21.11 · Fabric', 'uninstalled', '#42be65']));
            route = 'detail';
            refresh();
            byId('instance-heading')?.focus();
            speak('Instance created. Install when you are ready.');
            break;
        }
        case 'placeholder':
            open(target, `<p>This sketch shows where the command lives. See the review notes for the interaction.</p><div class="commands">${button('Close', 'close')}</div>`);
            break;
    }
}

preview.addEventListener('click', click);
modal.addEventListener('click', click);
modal.addEventListener('close', () => {
    if (opener?.isConnected) opener.focus();
});
preview.addEventListener('input', (event) => {
    if (event.target.id !== 'query') return;
    query = event.target.value;
    renderResults();
    window.clearTimeout(searchTimer);
    searchTimer = window.setTimeout(() => speak(byId('result-count')?.textContent || ''), 300);
});
preview.addEventListener('change', (event) => {
    if (event.target.id === 'filter') filter = event.target.value;
    else if (event.target.id === 'sort') sort = event.target.value;
    else return;
    renderResults();
    speak(byId('result-count').textContent);
});
byId('direction').addEventListener('change', reset);
byId('size').addEventListener('change', reset);
byId('width').addEventListener('change', () => { preview.dataset.width = byId('width').value; });
byId('theme').addEventListener('change', theme);
systemTheme.addEventListener('change', theme);
reset();
