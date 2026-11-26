export type Id = 'sample' | 'repeat' | 'overlap'
export type Phase = 'idle' | 'review' | 'staged' | 'interrupted'
export interface Instance {
  id: Id
  name: string
  generation: 'A' | 'B'
  phase: Phase
  healthy: boolean
  setting: number
  saveRevision: number
  review: { added: string[]; removed: string[]; retained: string[] } | null
}
export interface Snapshot {
  instances: Instance[]
  metrics: {
    sourceBytes: number
    reusedBytes: number
    objectBytes: number
    objectCount: number
    activeLogicalBytes: number
    allocatedObjectBytes: number | null
  }
  message: string
}
export type Action = {kind: 'createSample'|'createOverlap'|'createRepeat'} | {
  kind: 'review'|'stage'|'cancel'|'interrupt'|'resume'|'activate'|'rollback'|'repair'|'simulateMissing'|'simulateCorruption'|'editSetting'|'editSave'
  instance: Id
}

export function phaseLabel(p: Instance): string {
  if (!p.healthy) return 'Repair needed'
  return {idle:'Installed',review:'Review changes',staged:'Update staged',interrupted:'Update interrupted'}[p.phase]
}
export function activityLabel(instance: Instance): string {
  return !instance.healthy ? 'Repair needed' : {
    idle: 'Installed',
    review: 'Awaiting review',
    staged: 'Awaiting activation',
    interrupted: 'Resume or cancel'
  }[instance.phase]
}
export function kib(bytes: number | null) {
  return bytes === null ? 'Unavailable' : new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(bytes / 1024) + ' KiB'
}
