// SPDX-License-Identifier: MPL-2.0
(() => {
  const versions = ['26.3', '1.21.1', '1.20.4', '1.20.1', '24w14a'];
  const profiles = Array.from({ length: 100 }, (_, index) => ({
    id: '00000000-0000-4000-8000-' + String(index + 1).padStart(12, '0'),
    name: 'Personal ' + String(index + 1).padStart(3, '0'),
    version: versions[index % versions.length],
    memoryMiB: 4096,
    windowWidth: 1200,
    windowHeight: 720,
  }));
  const preview = {added: ['Rain B'], removed: ['Rain A'], retained: ['Forest']};
  const snapshot = {
    instances: [
      {id: 'sample', name: 'Forest Sample', phase: 'review', review: preview},
      {id: 'repeat', name: 'Forest Repeat', phase: 'idle', review: null},
      {id: 'overlap', name: 'Coast Sample', phase: 'interrupted', review: preview},
    ].map(item => ({...item, generation: 'A', healthy: true, setting: 1, saveRevision: 0})),
    metrics: {sourceBytes: 98304, reusedBytes: 147456, objectBytes: 98304,
      objectCount: 5, activeLogicalBytes: 245760, allocatedObjectBytes: 98304},
    message: 'Synthetic benchmark snapshot.',
  };

  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  let pending = 0;
  let maxPending = 0;
  window.__TAURI_INTERNALS__ = {
    transformCallback() { return 1; },
    async invoke(command, args) {
      if (command === 'plugin:event|listen') return 1;
      if (command === 'fixture_library' || command === 'personal_library') {
        pending++;
        maxPending = Math.max(pending, maxPending);
        await delay(80);
        pending--;
        return structuredClone(command === 'fixture_library' ? snapshot : profiles);
      }
      if (command === 'fixture_action' && args.action.instance === 'sample') {
        const review = args.action.kind === 'review';
        if (!review && args.action.kind !== 'cancel') throw Error(command + ': unknown action');
        if (!review) await delay(120);
        const result = structuredClone(snapshot);
        result.instances[0].phase = review ? 'review' : 'idle';
        result.instances[0].review = review ? preview : null;
        result.message = review ? 'Update review ready.' : 'Update cancelled.';
        return result;
      }
      throw Error('Unsupported benchmark command ' + command);
    },
  };

  function untilFrame(start, expected) {
    return new Promise((resolve, reject) => {
      function check() {
        if (expected()) {
          document.querySelector('.split').getBoundingClientRect();
          resolve(performance.now() - start);
        } else if (performance.now() - start > 3000) {
          reject(Error('Timed out waiting for expected UI state'));
        } else {
          requestAnimationFrame(check);
        }
      }
      requestAnimationFrame(check);
    });
  }

  const rowCount = () => document.querySelectorAll('.instance-entry').length;
  const activityCount = () => document.querySelectorAll('.activity .task').length;
  const button = label => [...document.querySelectorAll('button')]
    .find(item => item.textContent.trim() === label);
  function search(value, expected) {
    const input = document.querySelector('#search');
    const start = performance.now();
    input.value = value;
    input.dispatchEvent(new Event('input', {bubbles: true}));
    return untilFrame(start, () => rowCount() === expected);
  }
  async function select(id, name) {
    const row = document.getElementById('instance-' + id);
    const start = performance.now();
    row.click();
    return untilFrame(start, () => row.getAttribute('aria-current') === 'true'
      && document.querySelector('.instance-identity h2')?.textContent === name
      && document.activeElement?.textContent === name);
  }
  async function cancel() {
    const start = performance.now();
    button('Cancel update').click();
    const working = await untilFrame(start, () =>
      document.querySelector('.activity-top').textContent.includes('Working…'));
    const completed = await untilFrame(start, () =>
      document.querySelector('.notice').textContent.includes('Update cancelled.')
      && !button('Cancel update'));
    const reviewStart = performance.now();
    button('Review update').click();
    await untilFrame(reviewStart, () => !!button('Cancel update') && activityCount() === 2);
    return {working, completed};
  }

  window.runLibraryBenchmark = async () => {
    await document.fonts.ready;
    await untilFrame(performance.now(), () => rowCount() === 100);
    const terms = [['personal', 100], ['1.20.4', 20], ['Personal 057', 1], ['missing phrase', 0], ['', 100]];
    const searchMs = [];
    const selectMs = [];
    for (let repeat = 0; repeat < 13; repeat++) {
      for (const [query, count] of terms) {
        const ms = await search(query, count);
        if (repeat > 0) searchMs.push(ms);
      }
      const personal = profiles[(repeat * 7) % profiles.length];
      const personalMs = await select(personal.id, personal.name);
      if (repeat > 0) selectMs.push(personalMs);
    }
    button('Review tools').click();
    await untilFrame(performance.now(), () => rowCount() === 3);
    await select('sample', snapshot.instances[0].name);
    const cancelTimes = [];
    for (let repeat = 0; repeat < 13; repeat++) {
      const result = await cancel();
      if (repeat > 0) cancelTimes.push(result);
    }
    return {
      personalRows: profiles.length,
      fixtureRows: rowCount(),
      activityStates: activityCount(),
      maxPendingInitialReads: maxPending,
      searchMs, selectMs,
      cancelWorkingMs: cancelTimes.map(sample => sample.working),
      cancelCompleteMs: cancelTimes.map(sample => sample.completed),
    };
  };
})();
