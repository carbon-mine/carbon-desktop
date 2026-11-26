<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import type { Id, Action, Snapshot } from './library'
  import { phaseLabel, activityLabel, kib } from './library'
  import type { PersonalProfile, ProfileMode, SetupReport, GameFilesPlan, GameFilesReceipt, GameFilesProgress } from './personal'
  import markLight from '../design-system/inherited/brand/mark-light.svg'
  import markDark from '../design-system/inherited/brand/mark-dark.svg'
  import DesktopIcon from '../design-system/DesktopIcon.svelte'
  import '../design-system/inherited/tokens.css'
  import '../design-system/workspace.css'
  import './review.css'

  const narrowViewport = '(max-width: 46rem)'
  const profileTabs = ['Overview', 'Settings'] as const
  const reviewTabs = ['Updates', 'Storage', 'Diagnostics'] as const
  type Tab = typeof profileTabs[number] | typeof reviewTabs[number]
  type SetupAction = 'check' | 'choose' | 'install' | 'client'
  type GameAction = 'plan' | 'games' | 'verify'
  type WorkAction = SetupAction | GameAction | 'sandbox' | 'launchReview'
  type ThemePreference = 'system' | 'light' | 'dark'
  type MenuAction = 'new-profile' | 'import-pack' | 'focus-search' | 'show-activity' | `theme-${ThemePreference}`
  type ShortcutSource = 'native-menu' | 'keyboard'
  type JavaProgress = {profileId: string; receivedBytes: number; totalBytes: number}
  const setupCommands = {check: 'inspect_vanilla_profile', choose: 'select_profile_java', install: 'install_profile_java', client: 'import_vanilla_client'}
  const workingLabels: Record<WorkAction, string> = {
    check: 'Checking Minecraft and Java requirements…', choose: 'Checking the selected Java runtime…',
    install: 'Preparing managed JDK…', client: 'Verifying the selected Minecraft client JAR…',
    plan: 'Reviewing the game download…', games: 'Preparing game files…',
    verify: 'Verifying game files…', sandbox: 'Running the sandbox test…',
    launchReview: 'Verifying installed game files and managed Java…'
  }
  const filePhases: Record<string, string> = {
    verifying: 'Verifying shared game files…', downloading: 'Downloading game files…',
    activating: 'Recording verified game files…', verifyingView: 'Verifying instance files…',
    copyingView: 'Preparing the instance file layout…', activatingView: 'Activating the instance file layout…'
  }
  const runtimeLabels = {compatible: 'Compatible Java found', incompatible: 'Java version is incompatible', missing: 'No compatible Java found'}
  const installJdkLabels = {unsupported: 'Unavailable on this system', available: 'Download required', repairNeeded: 'Repair required', ready: 'Prepared in app storage'}
  const managedActions = {unsupported: '', available: 'Prepare managed JDK', repairNeeded: 'Repair managed JDK', ready: 'Verify managed JDK'}
  const clientLabels = {missing: 'Not imported', verified: 'Verified', repairNeeded: 'Needs repair'}
  const shortcutActions: Record<string, MenuAction> = {n: 'new-profile', o: 'import-pack', f: 'focus-search'}
  const recentShortcuts = new Map<MenuAction, {source: ShortcutSource; time: number}>()
  let view = $state<'library' | 'review'>('library')
  let profileTab = $state<typeof profileTabs[number]>('Overview')
  let reviewTab = $state<typeof reviewTabs[number]>('Updates')
  let fixtureStarted = $state(false)
  let library = $state<Snapshot | null>(null)
  let selected = $state<Id>('sample')
  let detailOpen = $state(false)
  let profiles = $state<PersonalProfile[]>([])
  let personalId = $state<string | null>(null)
  let personal = $derived(profiles.find(item => item.id === personalId))
  let setupReports = $state<Record<string, SetupReport>>({})
  let setupErrors = $state<Record<string, string>>({})
  let setupWork = $state<{ key: string; profileId: string; action: WorkAction; name: string } | null>(null)
  let javaProgress = $state<JavaProgress | null>(null)
  let fileProgress = $state<GameFilesProgress | null>(null)
  let filePlans = $state<Record<string, GameFilesPlan>>({})
  let fileReceipts = $state<Record<string, GameFilesReceipt>>({})
  let fileNeedsReview = $state<Record<string, boolean>>({})
  let cancelRequested = $state(false)
  let cancelError = $state('')
  let setupReport = $derived(personal ? setupReports[setupKey(personal)] : undefined)
  let gamePlan = $derived(personal && setupReport && filePlans[setupKey(personal)]?.metadataSha1 === setupReport.metadataSha1
    ? filePlans[setupKey(personal)] : undefined)
  let clientVerified = $derived(setupReport?.clientFile.status === 'verified')
  let gameReceipt = $derived(personal && setupReport?.gameFilesPrepared && clientVerified && fileReceipts[setupKey(personal)]?.metadataSha1 === setupReport.metadataSha1
    ? fileReceipts[setupKey(personal)] : undefined)
  let gamePrepared = $derived(clientVerified && !!setupReport?.gameFilesPrepared && !!personal && !fileNeedsReview[setupKey(personal)])
  let prepareFromCache = $derived(!!personal && clientVerified && !!setupReport?.gameFilesCached &&
    !setupReport.gameFilesPrepared && !fileNeedsReview[setupKey(personal)])
  let managedJdkReady = $derived(setupReport?.managedJava.status === 'ready')
  let setupError = $derived(personal ? setupErrors[setupKey(personal)] : '')
  let cachedReads = new Set<string>()
  const explicitSetups = new Set<string>()
  let nativePickerActive = $derived(setupWork?.action === 'choose' || setupWork?.action === 'client')
  let instance = $derived(library?.instances.find(item => item.id === selected))
  let query = $state('')
  let personalMatches = $derived(profiles.filter(p => (p.name + ' ' + p.version).toLowerCase().includes(query.trim().toLowerCase())))
  let matches = $derived(library?.instances.filter(item =>
    [item.name, phaseLabel(item)].some(text => text.toLowerCase().includes(query.trim().toLowerCase()))) ?? [])
  let tasks = $derived(library?.instances.filter(item => !item.healthy || item.phase !== 'idle') ?? [])
  let busy = $state(false)
  let fixtureLoading = $state(false)
  let personalLoading = $state(true)
  let personalReady = $state(false)
  let fixtureNeedsReload = $state(false)
  let personalNeedsReload = $state(false)
  let fixtureDisabled = $derived(busy || fixtureLoading || !library || fixtureNeedsReload)
  let personalDisabled = $derived(busy || !!setupWork || personalLoading || !personalReady || personalNeedsReload)
  let message = $state('')
  let fixtureFailure = $state<{ text: string; instance?: Id; targetVerification: boolean } | null>(null)
  let personalFailure = $state('')
  let packFailure = $state('')
  let selectedTargetFailure = $derived(view === 'review' && fixtureFailure?.targetVerification && fixtureFailure.instance === selected)
  let search = $state<HTMLInputElement>()
  let heading = $state<HTMLHeadingElement>()
  let fixtureAlertHeading = $state<HTMLHeadingElement>()
  let personalAlertHeading = $state<HTMLHeadingElement>()
  let packAlertHeading = $state<HTMLHeadingElement>()
  let libraryHeading = $state<HTMLHeadingElement>()
  let activityHeading = $state<HTMLHeadingElement>()
  let installReviewHeading = $state<HTMLHeadingElement>()
  let requirementsFor = $state<string | null>(null)
  let launchReview = $state<{ key: string; message: string } | null>(null)
  let canReviewInstalledInputs = $derived(!!personal && !personal.importedPack && !!(gamePrepared || gameReceipt))
  let primary = $state<HTMLButtonElement>()
  let workspace = $state<HTMLElement>()
  let theme = $state<ThemePreference>('system')
  let dark = $state(false)

  onMount(() => {
    const media = matchMedia('(prefers-color-scheme: dark)')
    const saved = localStorage.getItem('review-theme')
    if (saved === 'light' || saved === 'dark') theme = saved
    const update = () => applyTheme(media.matches)
    update()
    syncNativeTheme()
    if (document.documentElement.dataset.desktopPlatform === 'windows') {
      void document.fonts.load('16px "Carbon system icons"', '\uE710').then(fonts => {
        if (fonts.length) document.documentElement.dataset.nativeIcons = 'true'
      }).catch(() => {})
    }
    media.addEventListener('change', update)
    window.addEventListener('keydown', keyboardShortcut, true)
    void refreshPersonal()
    const listeners = [
      listen<MenuAction>('desktop-menu', ({payload}) => menuAction(payload, 'native-menu')),
      listen<JavaProgress>('java-progress', ({payload}) => {
        if (setupWork?.action === 'install' && payload.profileId === setupWork.profileId &&
            payload.totalBytes > 0 && payload.receivedBytes >= 0 && payload.receivedBytes <= payload.totalBytes) javaProgress = payload
      }),
      listen<GameFilesProgress>('game-files-progress', ({payload}) => {
        if ((setupWork?.action === 'games' || setupWork?.action === 'verify') && payload.profileId === setupWork.profileId) fileProgress = payload
      })
    ].map(listener => listener.catch(() => undefined))
    return () => {
      media.removeEventListener('change', update)
      window.removeEventListener('keydown', keyboardShortcut, true)
      for (const listener of listeners) void listener.then(remove => remove?.())
    }
  })

  function applyTheme(systemDark = matchMedia('(prefers-color-scheme: dark)').matches) {
    dark = theme === 'dark' || (theme === 'system' && systemDark)
    document.documentElement.dataset.carbonTheme = dark ? 'dark' : 'light'
  }

  function chooseTheme(preference: ThemePreference) {
    theme = preference
    localStorage.setItem('review-theme', theme)
    applyTheme()
    syncNativeTheme()
  }

  function syncNativeTheme() {
    void invoke('apply_desktop_theme', {preference: theme}).catch(() => {})
  }

  async function focusSearch() {
    if (matchMedia(narrowViewport).matches) detailOpen = false
    await tick()
    const target = search ?? libraryHeading
    target?.focus()
  }

  function keyboardShortcut(event: KeyboardEvent) {
    if (event.isComposing || event.altKey || event.shiftKey) return
    const modified = event.ctrlKey || event.metaKey
    const action = modified ? shortcutActions[event.key.toLowerCase()] : event.key === 'F6' ? 'show-activity' : undefined
    if (!action) return
    event.preventDefault()
    if (!event.repeat) menuAction(action, 'keyboard')
  }

  function menuAction(action: MenuAction, source: ShortcutSource) {
    if (busy || nativePickerActive) return
    const previous = recentShortcuts.get(action)
    const time = performance.now()
    // A native accelerator can reach the renderer both as a key and as a menu event.
    if (previous && previous.source !== source && time - previous.time < 200) return
    recentShortcuts.set(action, { source, time })
    if (action === 'new-profile') createProfile()
    if (action === 'import-pack') void importPack(document.activeElement instanceof HTMLElement ? document.activeElement : null)
    if (action === 'focus-search') void focusSearch()
    if (action === 'show-activity') activityHeading?.focus()
    if (action === 'theme-system') chooseTheme('system')
    if (action === 'theme-light') chooseTheme('light')
    if (action === 'theme-dark') chooseTheme('dark')
  }

  function changeTab(tab: Tab) {
    if (tab === 'Overview' || tab === 'Settings') profileTab = tab
    else reviewTab = tab
    workspace?.scrollTo({ top: 0 })
  }

  async function tabKeys(event: KeyboardEvent, tabs: readonly Tab[], tab: Tab) {
    const index = tabs.indexOf(tab)
    const next = {
      ArrowLeft: tabs[(index - 1 + tabs.length) % tabs.length],
      ArrowRight: tabs[(index + 1) % tabs.length],
      Home: tabs[0], End: tabs[tabs.length - 1]
    }[event.key]
    if (!next) return
    event.preventDefault()
    changeTab(next)
    await tick()
    document.getElementById('tab-' + next)?.focus()
  }

  async function showReadiness() {
    const profileId = personalId
    if (!profileId) return
    requirementsFor = profileId
    changeTab('Overview')
    await tick()
    document.getElementById('launch-readiness')?.focus()
  }

  async function reviewInstalledLaunchInputs() {
    const profile = personal
    if (!profile || !canReviewInstalledInputs) return
    const key = beginWork(profile, 'launchReview')
    if (!key) return
    let result: {message: string}
    try {
      result = await invoke<{ message: string }>('review_installed_launch_inputs', {profileId: profile.id})
    } catch (error) {
      result = { message: errorText(error) }
    } finally {
      finishWork()
    }
    const detailVisible = detailOpen || !matchMedia(narrowViewport).matches
    if (view === 'library' && detailVisible && profileTab === 'Overview' && personal && setupKey(personal) === key) {
      launchReview = { key, message: result.message }
    }
  }

  function errorText(reason: unknown) {
    return typeof reason === 'string' ? reason : 'The native command was unavailable. Reopen Carbon Desktop and retry.'
  }

  function describeFixtureFailure(reason: unknown, id?: Id, duringMutation = false) {
    const nativeMessage = errorText(reason)
    const targetVerification = nativeMessage.includes('target generation failed verification')
    const rejectedBeforeMutation = targetVerification ||
      nativeMessage.startsWith('This save was edited under generation B.') ||
      nativeMessage.startsWith('Activation is waiting for this instance')
    fixtureNeedsReload ||= duringMutation && !rejectedBeforeMutation
    const current = library?.instances.find(item => item.id === id)
    fixtureFailure = {
      instance: fixtureNeedsReload ? undefined : id,
      targetVerification,
      text: targetVerification && current?.healthy && !fixtureNeedsReload
        ? 'The target generation failed verification. Generation ' + current.generation + ' remains active. Repair files, then retry.'
        : nativeMessage + (fixtureNeedsReload ? ' Reload the fixture library to confirm its state before retrying.' : '')
    }
  }

  function chooseAvailableProfile() {
    if (!personalId && profiles.length) personalId = profiles[0].id
  }

  async function refreshFixture() {
    const reloading = !!fixtureFailure
    fixtureStarted = true
    fixtureLoading = true
    fixtureFailure = null
    try {
      library = await invoke<Snapshot>('fixture_library')
      selected = library.instances.find(item => item.id === selected)?.id ?? library.instances[0]?.id ?? 'sample'
      fixtureNeedsReload = false
    } catch (reason) {
      fixtureNeedsReload = true
      describeFixtureFailure(reason)
    } finally {
      fixtureLoading = false
    }
    chooseAvailableProfile()
    if (reloading) {
      await tick()
      if (fixtureFailure) fixtureAlertHeading?.focus()
      else if (detailOpen && view === 'review' && instance) heading?.focus()
      else (search ?? libraryHeading)?.focus()
    }
  }

  async function refreshPersonal() {
    const reloading = !!personalFailure
    launchReview = null
    personalLoading = true
    personalFailure = ''
    try {
      profiles = await invoke<PersonalProfile[]>('personal_library')
      personalReady = true
      personalNeedsReload = false
      if (!profiles.some(item => item.id === personalId)) personalId = null
    } catch (reason) {
      personalNeedsReload = true
      personalFailure = errorText(reason)
    } finally {
      personalLoading = false
    }
    chooseAvailableProfile()
    if (personal) void readCachedSetup(personal)
    if (reloading) {
      await tick()
      if (personalFailure) personalAlertHeading?.focus()
      else if (detailOpen && view === 'library' && personal) heading?.focus()
      else (search ?? libraryHeading)?.focus()
    }
  }

  async function focusDetail() {
    await tick()
    workspace?.scrollTo({ top: 0 })
    heading?.focus()
  }

  async function select(id: Id) {
    if (busy) return
    view = 'review'
    selected = id
    reviewTab = 'Updates'
    detailOpen = true
    await focusDetail()
  }

  async function backToLibrary() {
    detailOpen = false
    await tick()
    const entry = document.getElementById('instance-' + (view === 'review' ? selected : personalId)) ?? search ?? libraryHeading
    entry?.focus()
  }

  async function openView(target: 'library' | 'review') {
    if (target === view) return
    view = target
    query = ''
    detailOpen = false
    if (view === 'review' && !fixtureStarted) void refreshFixture()
    await tick()
    libraryHeading?.focus()
  }

  async function apply(action: Action) {
    if (fixtureDisabled) return
    const origin = document.activeElement as HTMLElement | null
    busy = true
    fixtureFailure = null
    message = ''
    let created = false
    try {
      const previous = library?.metrics.sourceBytes ?? 0
      const reused = library?.metrics.reusedBytes ?? 0
      library = await invoke<Snapshot>('fixture_action', { action })
      if (!('instance' in action)) {
        selected = ({ createSample:'sample', createOverlap:'overlap', createRepeat:'repeat' } as const)[action.kind]
        reviewTab = 'Updates'
        query = ''
        detailOpen = true
        created = true
      }
      const loaded = library.metrics.sourceBytes - previous
      const saved = library.metrics.reusedBytes - reused
      message = library.message + ((loaded || saved) ? ' Read ' + kib(loaded) + ' from the fixture and reused ' + kib(saved) + '.' : '')
    } catch (reason) {
      describeFixtureFailure(reason, 'instance' in action ? action.instance : undefined, true)
    } finally {
      busy = false
    }
    if (created) {
      await focusDetail()
      return
    }
    await tick()
    if (fixtureFailure) fixtureAlertHeading?.focus()
    else if (origin?.isConnected && !origin.matches(':disabled')) origin.focus()
    else (primary ?? heading)?.focus()
  }

  function change(kind: Extract<Action,{instance:Id}>['kind']) {
    void apply({ kind, instance: selected })
  }

  async function checkPlay(event: MouseEvent) {
    if (fixtureDisabled) return
    const origin = event.currentTarget as HTMLElement
    busy = true
    fixtureFailure = null
    try {
      const result = await invoke<{ available: boolean; reason: string }>('play_readiness', { instance: selected })
      message = result.reason
    } catch (reason) {
      describeFixtureFailure(reason)
    } finally {
      busy = false
    }
    await tick()
    const target = fixtureFailure ? fixtureAlertHeading : origin
    target?.focus()
  }

  async function importPack(opener: HTMLElement | null) {
    if (busy || nativePickerActive || setupWork?.action === 'launchReview') return
    busy = true
    packFailure = ''
    try {
      const imported = await invoke<{id: string} | null>('import_pack')
      if (imported) {
        await refreshPersonal()
        if (personalNeedsReload) return
        query = ''
        message = 'Imported the pack into your local library.'
        await selectPersonal(imported.id)
        return
      }
    } catch (reason) {
      packFailure = errorText(reason)
    } finally {
      busy = false
    }
    await tick()
    if (packFailure) packAlertHeading?.focus()
    else (opener?.isConnected ? opener : document.getElementById('new-profile'))?.focus()
  }

  async function selectPersonal(id: string) {
    view = 'library'
    if (personalId !== id) profileTab = 'Overview'
    personalId = id
    detailOpen = true
    if (personal) void readCachedSetup(personal)
    await focusDetail()
  }

  function packAwaitingSupport(profile: PersonalProfile) {
    return !!(profile.importedPack?.loader || profile.importedPack?.pendingFiles)
  }

  function profileSource(profile: PersonalProfile) {
    return profile.importedPack ? (profile.importedPack.loader || 'Imported pack') : 'Vanilla'
  }

  function setupKey(profile: PersonalProfile) {
    return profile.id + '/' + profile.version
  }

  const sizeFormat = new Intl.NumberFormat(undefined, {maximumFractionDigits: 1})
  function downloadSize(bytes: number) {
    return bytes < 1024 ** 2 ? kib(bytes) : sizeFormat.format(bytes / 1024 ** 2) + ' MiB'
  }

  function validateReport(profile: PersonalProfile, result: SetupReport) {
    if (result.profileId !== profile.id || result.version !== profile.version) {
      throw 'The profile version changed while checking setup. Select the profile and check it again.'
    }
  }

  async function readCachedSetup(profile: PersonalProfile) {
    const key = setupKey(profile)
    if (packAwaitingSupport(profile) || cachedReads.has(key)) return
    cachedReads.add(key)
    try {
      const report = await invoke<SetupReport>('inspect_vanilla_profile', {profileId: profile.id, offline: true})
      validateReport(profile, report)
      if (!explicitSetups.has(key) && !setupReports[key]) setupReports[key] = report
    } catch {
      // A cache miss on selection leaves Check setup available.
    }
  }

  function keepTaskFocus() {
    const task = document.activeElement?.closest<HTMLElement>('.instance-task')
    if (task) task.focus({preventScroll: true})
  }

  function beginWork(profile: PersonalProfile, action: WorkAction) {
    if (personalDisabled || packAwaitingSupport(profile)) return
    const key = setupKey(profile)
    if (action !== 'check' && action !== 'plan' && !setupReports[key]) return
    keepTaskFocus()
    launchReview = null
    explicitSetups.add(key)
    if (profileTab !== 'Overview') changeTab('Overview')
    setupWork = {key, profileId: profile.id, action, name: profile.name}
    javaProgress = null
    fileProgress = null
    cancelRequested = false
    cancelError = ''
    setupErrors[key] = ''
    if (action !== 'launchReview') message = profile.name + ' · ' + workingLabels[action]
    return key
  }

  function finishWork() {
    keepTaskFocus()
    setupWork = null
    javaProgress = null
    fileProgress = null
  }

  async function runSetup(profile: PersonalProfile, action: SetupAction) {
    const key = beginWork(profile, action)
    if (!key) return
    try {
      const args = action === 'check' ? {profileId: profile.id, offline: false} :
        action === 'install' ? {profileId: profile.id, metadataSha1: setupReports[key].metadataSha1} :
        {profileId: profile.id}
      const report = await invoke<SetupReport | null>(setupCommands[action], args)
      if (report) {
        validateReport(profile, report)
        setupReports[key] = report
        const result = action === 'client' ? 'Client JAR ' + clientLabels[report.clientFile.status].toLowerCase() :
          action === 'check' ? 'Version requirements verified' :
          action === 'install' ? 'Managed JDK ' + installJdkLabels[report.managedJava.status].toLowerCase() :
          runtimeLabels[report.runtime.state]
        message = profile.name + ' · ' + result + '.'
      } else {
        message = profile.name + (action === 'client' ? ' · Client JAR selection cancelled.' : ' · Java selection cancelled.')
      }
    } catch (reason) {
      const text = errorText(reason)
      const cancelled = action === 'install' && cancelRequested && text.includes('was cancelled')
      if (!cancelled) {
        delete setupReports[key]
        setupErrors[key] = text
      }
      message = profile.name + ' · ' + text
    } finally {
      finishWork()
    }
  }

  async function runGameSetup(profile: PersonalProfile, action: GameAction) {
    const confirmedPlan = gamePlan
    if (action === 'games' && !confirmedPlan) return
    const key = beginWork(profile, action)
    if (!key) return
    try {
      let current = setupReports[key]
      if (!current) {
        current = await invoke<SetupReport>('inspect_vanilla_profile', {profileId: profile.id, offline: false})
        validateReport(profile, current)
        setupReports[key] = current
      }
      if (action === 'games' && current.managedJava.status !== 'unsupported' && current.managedJava.status !== 'ready') {
        setupWork = {key, profileId: profile.id, action: 'install', name: profile.name}
        javaProgress = null
        message = profile.name + ' · Preparing the managed JDK…'
        const prepared = await invoke<SetupReport>('install_profile_java', {
          profileId: profile.id, metadataSha1: confirmedPlan?.metadataSha1
        })
        validateReport(profile, prepared)
        setupReports[key] = prepared
        current = prepared
        if (prepared.metadataSha1 !== confirmedPlan?.metadataSha1) {
          throw 'Minecraft metadata changed. Review the download again before installing.'
        }
        if (cancelRequested) {
          message = profile.name + ' · JDK prepared. Game file download cancelled.'
          return
        }
        javaProgress = null
        setupWork = {key, profileId: profile.id, action: 'games', name: profile.name}
        message = profile.name + ' · Preparing Minecraft game files…'
      }
      const args = {profileId: profile.id, offline: action === 'verify'}
      const result = action === 'plan'
        ? await invoke<GameFilesPlan>('plan_game_files', args)
        : await invoke<GameFilesReceipt>('prepare_game_files', {
            ...args, metadataSha1: action === 'games' ? confirmedPlan?.metadataSha1 : current.metadataSha1
          })
      if (result.profileId !== profile.id || result.version !== profile.version || result.metadataSha1 !== current.metadataSha1) {
        throw 'Minecraft setup changed. Recheck the profile before preparing files.'
      }
      if ('libraryFiles' in result) {
        filePlans[key] = result
        message = profile.name + ' · Game download ready for review.'
      } else {
        fileReceipts[key] = result
        fileNeedsReview[key] = false
        delete filePlans[key]
        await refreshPreparedProfile(profile, key, result)
      }
    } catch (reason) {
      const text = errorText(reason)
      if (!(cancelRequested && /cancell?ed/i.test(text))) {
        setupErrors[key] = text
        if (setupWork?.action === 'install') delete setupReports[key]
        if (action === 'verify') {
          fileNeedsReview[key] = true
          delete fileReceipts[key]
        }
      }
      message = profile.name + ' · ' + text
    } finally {
      finishWork()
      if (action === 'plan' && filePlans[key] && view === 'library' && profileTab === 'Overview' && personalId === profile.id) {
        await tick()
        installReviewHeading?.focus()
      }
    }
  }

  async function refreshPreparedProfile(profile: PersonalProfile, key: string, receipt: GameFilesReceipt) {
    try {
      const refreshed = await invoke<SetupReport>('inspect_vanilla_profile', {profileId: profile.id, offline: true})
      validateReport(profile, refreshed)
      if (refreshed.metadataSha1 !== receipt.metadataSha1) throw 'The stored version metadata changed.'
      setupReports[key] = refreshed
      message = profile.name + ' · ' + receipt.uniqueFiles + ' game files verified.'
    } catch (reason) {
      // Keep the completed receipt for the verified version. Hide it until profile setup confirms that version again.
      delete setupReports[key]
      const text = receipt.uniqueFiles + ' game files verified for Minecraft ' + receipt.version +
        '. The current profile could not be confirmed. Recheck setup. ' + errorText(reason)
      setupErrors[key] = text
      message = profile.name + ' · ' + text
    }
  }

  async function runSandboxTest(profile: PersonalProfile) {
    const current = setupReports[setupKey(profile)]
    if (!current?.sessionSupported || current.managedJava.status !== 'ready') return
    const key = beginWork(profile, 'sandbox')
    if (!key) return
    try {
      const result = await invoke<{sampleCount: number; saved: boolean}>('run_sandbox_session', {profileId: profile.id})
      message = profile.name + (result.saved ? ' · Sandbox sample saved. Count ' + result.sampleCount + '.' : ' · Sandbox test closed without saving.')
    } catch (reason) {
      setupErrors[key] = errorText(reason)
      message = profile.name + ' · ' + setupErrors[key]
    } finally {
      finishWork()
    }
  }

  async function cancelInstall(profileId: string) {
    const action = setupWork?.action
    if (cancelRequested || setupWork?.profileId !== profileId || !action || !['install', 'games', 'verify'].includes(action)) return
    cancelRequested = true
    cancelError = ''
    try {
      await invoke(action === 'install' ? 'cancel_java_install' : 'cancel_game_files', {profileId})
    } catch (reason) {
      if (setupWork?.profileId === profileId) {
        cancelRequested = false
        cancelError = errorText(reason)
      }
    }
  }

  async function openGameFolder(profile: PersonalProfile) {
    if (personalDisabled) return
    busy = true
    try {
      await invoke('open_game_folder', {profileId: profile.id})
    } catch (reason) {
      message = profile.name + ' · ' + errorText(reason)
    } finally {
      busy = false
    }
  }

  async function showSetupActivity(profileId: string) {
    await selectPersonal(profileId)
    changeTab('Overview')
  }

  function createProfile() {
    void editProfile('create', document.getElementById('new-profile'))
  }

  async function editProfile(mode: ProfileMode, opener: HTMLElement | null) {
    if (personalDisabled) return
    busy = true
    personalFailure = ''
    try {
      const result = await invoke<{profiles: PersonalProfile[]; selectedId: string} | null>('edit_profile_dialog', {
        mode, profileId: mode === 'create' ? null : personalId
      })
      if (!result) return
      launchReview = null
      profiles = result.profiles
      query = ''
      message = mode === 'clone' ? 'Created an independent copy of the profile settings.' : 'Saved the personal profile on this device.'
      await selectPersonal(result.selectedId)
    } catch (e) {
      personalNeedsReload = true
      personalFailure = errorText(e) + ' Reload personal profiles to confirm their state before retrying.'
      await tick()
      personalAlertHeading?.focus()
    } finally {
      busy = false
      await tick()
      if (!personalFailure && document.activeElement !== heading) opener?.focus()
    }
  }
</script>

<div class="desktop">
  <header class="app-header">
    <div class="command-bar" aria-label="Launcher commands">
      <button class="toolbar-button" id="new-profile" disabled={personalDisabled} onclick={createProfile}><DesktopIcon name="add" /> New instance</button>
    </div>
    <div class="app-options">
      <span class="account"><DesktopIcon name="account" /> Signed out</span>
    </div>
  </header>

  <div class="notices">
      {#if view === 'review' && fixtureFailure && (!fixtureFailure.instance || fixtureFailure.instance === selected)}
        <div class="error" role="alert">
          <h2 tabindex="-1" bind:this={fixtureAlertHeading}>Fixture library needs attention</h2>
          <p>{fixtureFailure.text}</p>
          <div class="actions">
            {#if selectedTargetFailure && instance?.healthy}<button disabled={fixtureDisabled} onclick={() => change('repair')}>Repair files</button>{/if}
            <button disabled={busy || fixtureLoading} onclick={refreshFixture}>Reload library</button>
          </div>
        </div>
      {/if}
      {#if view === 'library' && personalFailure}
        <div class="error" role="alert">
          <h2 tabindex="-1" bind:this={personalAlertHeading}>Personal profiles need attention</h2>
          <p>{personalFailure}</p>
          <button disabled={busy || personalLoading || !!setupWork} onclick={refreshPersonal}>Reload personal profiles</button>
        </div>
      {/if}
      {#if packFailure}
        <div class="error" role="alert">
          <h2 tabindex="-1" bind:this={packAlertHeading}>Could not import the pack</h2>
          <p>{packFailure}</p>
          <button disabled={busy || nativePickerActive} onclick={e => importPack(e.currentTarget)}>Choose another pack</button>
        </div>
      {/if}
  </div>

  <div class="split" class:has-instances={view === 'library' ? !!personal : !!instance} class:detail-open={detailOpen}>
    <section class="library-pane" aria-labelledby="library-heading">
      <div class="library-header">
        <h1 id="library-heading" tabindex="-1" bind:this={libraryHeading}>{view === 'library' ? 'Local instances' : 'Review fixtures'}</h1>
        <span class="counter">{view === 'library' ? profiles.length : library?.instances.length ?? 0}</span>
      </div>
      <div class="search-wrap">
        <DesktopIcon name="search" />
        <input aria-label="Find an instance" id="search" type="search" bind:this={search} bind:value={query} placeholder="Find an instance" autocomplete="off" />
      </div>
      <ul class="instances" aria-label={view === 'library' ? 'Local instances' : 'Review instances'}>
        {#if view === 'library'}
          {#each personalMatches as item (item.id)}
            <li><button id={'instance-' + item.id} class="instance-entry" aria-current={item.id === personalId ? 'true' : undefined} onclick={() => selectPersonal(item.id)}>
              <span class="instance-mark personal-mark" aria-hidden="true"><i></i></span>
              <span class="entry-label"><strong>{item.name}</strong><small>{profileSource(item)} {item.version}</small></span>
            </button></li>
          {:else}
            {#if personalLoading}<li class="empty-results">Opening library…</li>
            {:else if profiles.length}<li class="empty-results">No matching instances</li>
            {:else if personalReady}<li class="empty-results">No local instances</li>{/if}
          {/each}
        {:else}
          {#each matches as item (item.id)}
            <li><button id={'instance-' + item.id} class="instance-entry" aria-current={item.id === selected ? 'true' : undefined} disabled={busy} onclick={() => select(item.id)}>
              <span class="instance-mark fixture-mark" class:coast={item.id === 'overlap'} class:repeat={item.id === 'repeat'} aria-hidden="true"><i></i></span>
              <span class="entry-label"><strong>{item.name}</strong><small>Generation {item.generation} · {phaseLabel(item)}</small></span>
            </button></li>
          {:else}
            {#if fixtureLoading}<li class="empty-results">Opening fixtures…</li>
            {:else if library?.instances.length}<li class="empty-results">No matching fixtures</li>
            {:else if library}<li class="empty-results">No review fixtures</li>{/if}
          {/each}
        {/if}
      </ul>
      <nav class="view-switcher" aria-label="Launcher sections">
        <button aria-current={view === 'library' ? 'page' : undefined} onclick={() => openView('library')}><DesktopIcon name="library" /> Library</button>
        <button aria-current={view === 'review' ? 'page' : undefined} onclick={() => openView('review')}><DesktopIcon name="review" /> Review tools</button>
      </nav>
    </section>

    <main class="detail-pane" bind:this={workspace} aria-label={view === 'library' ? 'Instance workspace' : 'Review workspace'}>
      {#if (view === 'library' && personal) || (view === 'review' && instance)}
        <button class="narrow-back quiet-button" onclick={backToLibrary}><DesktopIcon name="back" /> {view === 'library' ? 'Back to Library' : 'Back to Review tools'}</button>
      {/if}
      {#if view === 'library'}
        {#if personal}
          <div class="instance-header">
            <span class="instance-mark personal-mark large-mark" aria-hidden="true"><i></i></span>
            <div class="instance-identity"><h2 bind:this={heading} tabindex="-1">{personal.name}</h2><p>Minecraft Java Edition · <span class="version-label">{profileSource(personal)} {personal.version}</span></p></div>
            <div class="launch-control">
              <button class="play-action" aria-describedby="play-hint" onclick={showReadiness}><DesktopIcon name="play" /> Play</button>
              <p id="play-hint">{packAwaitingSupport(personal) ? 'Pack preparation pending' : prepareFromCache ? 'Instance files required' : gamePrepared || gameReceipt ? (managedJdkReady ? 'Launch unavailable' : 'Java required') : 'Installation required'}</p>
            </div>
          </div>
          <div class="instance-tabs" role="tablist" aria-label="Instance">
            {#each profileTabs as tab}
              <button type="button" id={'tab-' + tab} role="tab" aria-selected={profileTab === tab} aria-controls={'panel-' + tab} tabindex={profileTab === tab ? 0 : -1} onclick={() => changeTab(tab)} onkeydown={e => tabKeys(e, profileTabs, tab)}>{tab}</button>
            {/each}
          </div>
          {#each profileTabs as panel}
          <div class="tab-content" id={'panel-' + panel} role="tabpanel" aria-labelledby={'tab-' + panel} tabindex="0" hidden={profileTab !== panel}>
            {#if profileTab === panel}
            {#if profileTab === 'Overview'}
              <section class="instance-task" aria-label="Current instance task" tabindex="-1">
                {#if packAwaitingSupport(personal)}
                  <div class="task-copy"><h3>Pack preparation pending</h3>
                    <p>{personal.importedPack?.name} requires additional pack content before it can play.</p></div>
                {:else if setupWork?.key === setupKey(personal) && setupWork.action !== 'launchReview'}
                  <div class="task-copy">
                    <h3>{cancelRequested ? 'Cancellation requested…' : javaProgress && javaProgress.receivedBytes === javaProgress.totalBytes ? 'Verifying and preparing Java…' : fileProgress ? (filePhases[fileProgress.phase] ?? workingLabels[setupWork.action]) : workingLabels[setupWork.action]}</h3>
                    {#if javaProgress && javaProgress.receivedBytes < javaProgress.totalBytes}
                      <progress value={javaProgress.receivedBytes} max={javaProgress.totalBytes} aria-label="Managed Java download"></progress>
                      <p class="technical">{kib(javaProgress.receivedBytes)} / {kib(javaProgress.totalBytes)}</p>
                    {:else if fileProgress && fileProgress.totalFiles > 0}
                      <progress value={fileProgress.completedFiles} max={fileProgress.totalFiles} aria-label="Minecraft game files"></progress>
                      <p class="technical">{fileProgress.completedFiles} / {fileProgress.totalFiles} files · {kib(fileProgress.verifiedBytes)} / {kib(fileProgress.totalBytes)}</p>
                    {/if}
                    {#if cancelError}<p role="alert">{cancelError}</p>{/if}
                  </div>
                  {#if setupWork.action === 'install' || setupWork.action === 'games' || setupWork.action === 'verify'}
                    <button disabled={cancelRequested} onclick={() => cancelInstall(personal!.id)}>{setupWork.action === 'install' ? 'Cancel Java preparation' : 'Cancel file preparation'}</button>
                  {/if}
                {:else if gamePlan}
                  <div class="task-copy"><h3 tabindex="-1" bind:this={installReviewHeading}>Install review</h3>
                    <p>Includes {gamePlan.libraryFiles} libraries and {gamePlan.assetObjects} assets. Carbon reuses verified files.</p></div>
                  <button class="primary" disabled={personalDisabled} onclick={() => runGameSetup(personal!, 'games')}>{setupReport?.managedJava.status === 'unsupported' ? 'Download game files' : 'Install instance'}</button>
                  <dl class="review-sizes">
                    <div><dt>Minecraft {gamePlan.version} · Mojang</dt><dd>{gamePlan.uniqueFiles} files · up to {downloadSize(gamePlan.totalBytes)}</dd></div>
                    <div><dt>Java {setupReport?.javaMajor} · Eclipse Temurin JDK</dt><dd>{setupReport?.managedJava.status === 'ready' ? 'Prepared in app storage' : setupReport?.managedJava.downloadBytes ? 'Up to ' + downloadSize(setupReport.managedJava.downloadBytes) : 'Managed JDK unavailable on this system'}</dd></div>
                  </dl>
                {:else if gamePrepared || gameReceipt}
                  <div class="task-copy"><h3>Game files ready</h3>
                    <p>{gameReceipt ? gameReceipt.uniqueFiles + ' files verified · ' + downloadSize(gameReceipt.totalBytes) : 'Previously prepared on this device.'}</p></div>
                  <button disabled={personalDisabled} onclick={() => runGameSetup(personal!, 'verify')}>Verify game files</button>
                {:else if prepareFromCache}
                  <div class="task-copy"><h3>Finish preparing this instance</h3>
                    <p>Verify the cached Minecraft files and prepare this instance offline.</p></div>
                  <button class="primary" disabled={personalDisabled} onclick={() => runGameSetup(personal!, 'verify')}>Prepare from cache</button>
                {:else}
                  <div class="task-copy"><h3>{personal && fileNeedsReview[setupKey(personal)] ? 'Repair game files' : 'Install Minecraft ' + personal.version}</h3>
                    <p>Review the required Java runtime and Minecraft files before downloading.</p></div>
                  <button class="primary" disabled={personalDisabled} onclick={() => runGameSetup(personal!, 'plan')}>Review install</button>
                {/if}
                {#if setupError}<p class="task-error" role="alert">{setupError}</p>{/if}
                {#if gamePrepared || gameReceipt || requirementsFor === personal.id || packAwaitingSupport(personal)}
                  <div class="readiness"><h4 id="launch-readiness" tabindex="-1">Play requirements</h4>
                    {#if setupWork?.action === 'launchReview'}
                      <p role="status">{setupWork.profileId === personal.id ? workingLabels.launchReview : 'A launch input review is in progress.'}</p>
                    {:else if launchReview?.key === setupKey(personal)}
                      <p role="status">{launchReview.message}</p>
                    {/if}
                    {#if canReviewInstalledInputs}
                      <button disabled={personalDisabled} onclick={reviewInstalledLaunchInputs}>
                        Review installed launch inputs
                      </button>
                    {:else if !personal.importedPack}
                      <p>Prepare this instance to review its installed launch inputs.</p>
                    {/if}
                    {#if setupReport?.blockedReasons.length}
                      {#each setupReport.blockedReasons as reason}<p>{reason}</p>{/each}
                    {:else}<p>Microsoft sign-in and protected Minecraft launch are pending.</p>{/if}
                    {#if (gamePrepared || gameReceipt) && setupReport && !managedJdkReady}
                      <p>Java {setupReport.javaMajor} · {installJdkLabels[setupReport.managedJava.status]}</p>
                    {/if}
                  </div>
                {/if}
              </section>
              {#if personal.importedPack}
                <dl class="property-list">
                  <div><dt>Pack</dt><dd>{personal.importedPack.name}</dd></div>
                  <div><dt>Loader</dt><dd>{personal.importedPack.loader ?? 'Vanilla'}</dd></div>
                  <div><dt>Files awaiting download</dt><dd>{personal.importedPack.pendingFiles}</dd></div>
                </dl>
              {/if}
              {#if !packAwaitingSupport(personal)}
                <details class="advanced-setup"><summary>Advanced setup and diagnostics</summary>
                  {#if setupReport}
                  <dl class="property-list">
                    <div><dt>Minecraft version</dt><dd>Verified · {setupReport.releaseType}{setupReport.origin === 'cache' ? ' · local cache' : ''}</dd></div>
                    {#if gameReceipt}<div><dt>Verified game files reused</dt><dd>{downloadSize(gameReceipt.reusedBytes)}</dd></div>{/if}
                    <div><dt>Metadata SHA-1</dt><dd class="technical">{setupReport.metadataSha1}</dd></div>
                    <div><dt>Client size listed</dt><dd>{kib(setupReport.clientBytes)}</dd></div>
                    <div><dt>Asset index size listed</dt><dd>{kib(setupReport.assetIndexBytes)}</dd></div>
                    <div><dt>Library entries listed</dt><dd>{setupReport.libraryCount}</dd></div>
                    <div><dt>Managed JDK</dt><dd>Java {setupReport.javaMajor} · {installJdkLabels[setupReport.managedJava.status]}{setupReport.managedJava.release ? ' · ' + setupReport.managedJava.release : ''}</dd></div>
                    {#if setupReport.managedJava.downloadBytes}<div><dt>Managed JDK download size</dt><dd>{downloadSize(setupReport.managedJava.downloadBytes)}</dd></div>{/if}
                    <div><dt>Runtime detection</dt><dd>{runtimeLabels[setupReport.runtime.state]}{setupReport.runtime.version ? ' · ' + setupReport.runtime.version : ''}{setupReport.runtime.source ? ' · ' + setupReport.runtime.source : ''}</dd></div>
                    <div><dt>Local client JAR</dt><dd>{clientLabels[setupReport.clientFile.status]}</dd></div>
                  </dl>
                  {/if}
                  <div class="diagnostic-actions">
                    <button disabled={personalDisabled} onclick={() => runSetup(personal!, 'check')}>{setupReport ? 'Recheck setup' : 'Check setup'}</button>
                    {#if setupReport}
                    {#if setupReport.managedJava.status !== 'unsupported'}
                      <button disabled={personalDisabled} onclick={() => runSetup(personal!, 'install')}>{managedActions[setupReport.managedJava.status]}</button>
                    {/if}
                    <button disabled={personalDisabled} onclick={() => runSetup(personal!, 'choose')}>Choose Java folder</button>
                    <button disabled={personalDisabled} onclick={() => runSetup(personal!, 'client')}>{setupReport.clientFile.status === 'verified' ? 'Verify client JAR' : 'Import client JAR'}</button>
                    {#if setupReport.sessionSupported && setupReport.managedJava.status === 'ready'}
                      <button disabled={personalDisabled} onclick={() => runSandboxTest(personal!)}>Open sandbox test</button>
                    {/if}
                    {/if}
                  </div>
                  {#if setupReport}<p>A local client JAR is optional. The sandbox test opens a separate window and writes only sample saves.</p>{/if}
                </details>
              {/if}
            {:else}
              <div class="section-heading"><h3>Instance settings</h3><button disabled={personalDisabled} onclick={e => editProfile('edit', e.currentTarget)}>Edit profile</button></div>
              <dl class="property-list">
                <div><dt>Minecraft version</dt><dd>{personal.version}</dd></div>
                <div><dt>Maximum memory</dt><dd>{personal.memoryMiB} MiB</dd></div>
                <div><dt>Starting window</dt><dd>{personal.windowWidth} × {personal.windowHeight}</dd></div>
              </dl>
              <div class="actions"><button disabled={personalDisabled} onclick={() => openGameFolder(personal!)}>Open game folder</button></div>
              {#if personal.importedPack}
                <p class="section-help">The pack defines its Minecraft version. Import the pack again to create an independent copy.</p>
                <div class="actions"><button disabled={personalDisabled} onclick={e => importPack(e.currentTarget)}>Import pack…</button></div>
              {:else}
              <div class="section-heading separated"><h3>Copy settings</h3>
                <button disabled={personalDisabled} onclick={e => editProfile('clone', e.currentTarget)}>Clone profile</button>
              </div>
              <p class="section-help">Creates a separate instance with its own version and launch settings.</p>
              {/if}
            {/if}
            {/if}
          </div>
          {/each}
        {:else if personalLoading}
          <p class="waiting">Opening the library…</p>
        {:else if personalReady}
          <section class="empty-workspace">
            <span class="instance-mark personal-mark welcome-mark" aria-hidden="true"><img alt="" src={dark ? markLight : markDark} /></span>
            <h2>Set up your first instance</h2>
            <p>Choose a Minecraft Java version or import a local pack.</p>
            <div class="actions">
              <button class="primary" onclick={createProfile} disabled={personalDisabled}>Create vanilla profile</button>
              <button disabled={personalDisabled} onclick={e => importPack(e.currentTarget)}>Import pack…</button>
            </div>
          </section>
        {/if}
      {:else if fixtureLoading}
        <p class="waiting">Opening the review fixtures…</p>
      {:else if instance && library}
        <div class="instance-header">
          <span class="instance-mark fixture-mark large-mark" aria-hidden="true"><i></i></span>
          <div class="instance-identity"><h2 id="instance-heading" bind:this={heading} tabindex="-1">{instance.name}</h2><p>Synthetic fixture · Generation {instance.generation}</p></div>
          <span class:warning={!instance.healthy || instance.phase === 'interrupted'} class="tag">{phaseLabel(instance)}</span>
        </div>
        <div class="instance-tabs" role="tablist" aria-label="Fixture">
          {#each reviewTabs as tab}
            <button type="button" id={'tab-' + tab} role="tab" aria-selected={reviewTab === tab} aria-controls={'panel-' + tab} tabindex={reviewTab === tab ? 0 : -1} onclick={() => changeTab(tab)} onkeydown={e => tabKeys(e, reviewTabs, tab)}>{tab}</button>
          {/each}
        </div>
        {#each reviewTabs as panel}
        <div class="tab-content" id={'panel-' + panel} role="tabpanel" aria-labelledby={'tab-' + panel} tabindex="0" hidden={reviewTab !== panel}>
          {#if reviewTab === panel}
          {#if reviewTab === 'Updates'}
            <h3>Fixture update</h3>
            <p class="section-help">Generation {instance.generation} stays active until its replacement passes verification and activation.</p>
            <div class="actions primary-actions">
              {#if !instance.healthy}
                <button class="primary" bind:this={primary} disabled={fixtureDisabled} onclick={() => change('repair')}>Repair files</button>
              {:else}
                {#if instance.phase === 'review'}
                  <button class="primary" bind:this={primary} disabled={fixtureDisabled} onclick={() => change('stage')}>Stage update</button>
                {:else if instance.phase === 'staged'}
                  <button class="primary" bind:this={primary} disabled={fixtureDisabled} onclick={() => change('activate')}>Activate B</button>
                {:else if instance.phase === 'interrupted'}
                  <button class="primary" bind:this={primary} disabled={fixtureDisabled} onclick={() => change('resume')}>Resume update</button>
                {:else if instance.generation === 'A'}
                  <button class="primary" bind:this={primary} disabled={fixtureDisabled} onclick={() => change('review')}>Review update</button>
                {:else}
                  <button bind:this={primary} disabled={fixtureDisabled} onclick={() => change('rollback')}>Roll back to A</button>
                {/if}
                {#if !selectedTargetFailure}<button disabled={fixtureDisabled} onclick={() => change('repair')}>Repair files</button>{/if}
              {/if}
              <button disabled={fixtureDisabled} onclick={checkPlay}>Check Play requirements</button>
            </div>
            {#if instance.review}
              <section class="separated">
                <h3>Generation A → B</h3>
                <dl class="property-list"><div><dt>Add</dt><dd>{instance.review.added.join(', ')}</dd></div>
                  <div><dt>Remove</dt><dd>{instance.review.removed.join(', ')}</dd></div><div><dt>Reuse</dt><dd>{instance.review.retained.join(', ')}</dd></div>
                </dl>
                <div class="actions">
                  <button disabled={fixtureDisabled} onclick={() => change('cancel')}>Cancel update</button>
                  {#if instance.phase === 'review' || instance.phase === 'staged'}
                    <button disabled={fixtureDisabled} onclick={() => change('interrupt')}>Simulate interruption</button>
                  {/if}
                </div>
              </section>
            {/if}
          {:else if reviewTab === 'Storage'}
            <h3>Content reuse</h3>
            <p class="section-help">Forest Repeat uses the same objects. Coast shares 80% of Forest’s immutable bytes. Each has its own sample save and settings.</p>
            <dl class="property-list">
              <div><dt>Fixture bytes read</dt><dd>{kib(library.metrics.sourceBytes)}</dd></div>
              <div><dt>Bytes reused</dt><dd>{kib(library.metrics.reusedBytes)}</dd></div>
              <div><dt>Object disk allocation</dt><dd>{kib(library.metrics.allocatedObjectBytes)}</dd></div>
              <div><dt>Active logical size</dt><dd>{kib(library.metrics.activeLogicalBytes)}</dd></div>
            </dl>
            <div class="actions">
              <button disabled={fixtureDisabled || library.instances.some(item => item.id === 'repeat')} onclick={() => apply({kind:'createRepeat'})}>Install exact repeat</button>
              <button disabled={fixtureDisabled || library.instances.some(item => item.id === 'overlap')} onclick={() => apply({kind:'createOverlap'})}>Install Coast sample</button>
            </div>
          {:else}
            <h3>Private fixture data</h3>
            <dl class="property-list"><div><dt>Sample setting</dt><dd>{instance.setting}</dd></div><div><dt>Save revision</dt><dd>{instance.saveRevision}</dd></div></dl>
            <p class="section-help">Editing a save under B requires B to reopen it.</p>
            <div class="actions"><button disabled={fixtureDisabled} onclick={() => change('editSetting')}>Change sample setting</button><button disabled={fixtureDisabled} onclick={() => change('editSave')}>Edit sample save</button></div>
            <section class="separated">
              <h3>Fault simulation</h3><p class="section-help">These affect shared fixture content. Repair restores it for every sample using those bytes.</p>
              <div class="actions"><button disabled={fixtureDisabled} onclick={() => change('simulateMissing')}>Simulate missing file</button><button disabled={fixtureDisabled} onclick={() => change('simulateCorruption')}>Simulate corrupt file</button></div>
            </section>
          {/if}
          {/if}
        </div>
        {/each}
      {:else if library}
        <section class="empty-workspace">
          <span class="instance-mark fixture-mark welcome-mark" aria-hidden="true"><i></i></span>
          <h2>Open a review fixture</h2>
          <p>Use synthetic content to check update recovery, storage reuse and isolated sample saves.</p>
          <button class="primary" disabled={fixtureDisabled} onclick={() => apply({kind:'createSample'})}>Install Forest sample</button>
        </section>
      {/if}
    </main>
  </div>

  <aside class="activity" aria-labelledby="activity-heading">
    <div class="activity-top">
      <h2 id="activity-heading" bind:this={activityHeading} tabindex="-1">Activity</h2>
      {#if setupWork}
        <button class="task" onclick={() => showSetupActivity(setupWork!.profileId)}>
          <strong>{setupWork.name}</strong><span>{workingLabels[setupWork.action]}</span>
        </button>
      {:else if busy}<span>Working…</span>
      {:else if view === 'review' && fixtureLoading}<span>Checking fixture activity…</span>
      {:else if view === 'review' && fixtureNeedsReload}<span>Reload to check fixture activity</span>
      {:else if view !== 'review' || !tasks.length}<span>No active tasks</span>{/if}
      {#if view === 'review'}
        {#each tasks as task (task.id)}
          <button class="task" disabled={busy} onclick={() => select(task.id)}>
            <strong>{task.name}</strong><span>{activityLabel(task)}</span>
          </button>
        {/each}
      {/if}
    </div>
    <p class="notice" role="status" aria-atomic="true">{message}</p>
  </aside>
</div>
