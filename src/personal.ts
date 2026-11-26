export interface ProfileSettings {
  name: string
  version: string
  memoryMiB: number
  windowWidth: number
  windowHeight: number
}
export interface PersonalProfile extends ProfileSettings {
  id: string
  importedPack?: {name: string; loader: string | null; pendingFiles: number} | null
  serverAddress?: string
  serverJoinMessage?: string
}
export type ProfileMode = 'create' | 'edit' | 'clone'

export interface SetupReport {
  profileId: string
  version: string
  releaseType: string
  javaMajor: number
  clientBytes: number
  assetIndexBytes: number
  libraryCount: number
  metadataSha1: string
  origin: 'network' | 'cache'
  runtime: {
    state: 'compatible' | 'incompatible' | 'missing'
    version: string | null
    source: string | null
  }
  managedJava: {
    status: 'unsupported' | 'available' | 'repairNeeded' | 'ready'
    major: number
    release?: string
    downloadBytes?: number
  }
  clientFile: {status: 'missing' | 'verified' | 'repairNeeded'}
  gameFilesCached: boolean
  gameFilesPrepared: boolean
  sessionSupported: boolean
  blockedReasons: string[]
}

interface GameFilesIdentity {
  profileId: string
  version: string
  platform: string
  metadataSha1: string
  uniqueFiles: number
  totalBytes: number
}

export interface GameFilesPlan extends GameFilesIdentity {
  libraryFiles: number
  assetObjects: number
  assetNames: number
  loggingFiles: number
}

export interface GameFilesReceipt extends GameFilesIdentity {
  fetchedBytes: number
  reusedBytes: number
}

export interface GameFilesProgress {
  profileId: string
  completedFiles: number
  totalFiles: number
  verifiedBytes: number
  totalBytes: number
  phase: string
}
