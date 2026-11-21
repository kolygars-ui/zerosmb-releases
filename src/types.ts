export interface SmbConfig {
  host: string;
  share: string;
  port: number;
  domain: string;
  username: string;
  basePath: string;
}

export type SyncSource =
  | { kind: "localPath"; value: string }
  | { kind: "androidTree"; value: string };

export type SyncMode =
  | "upload_manual"
  | "download_manual"
  | "upload_auto"
  | "download_auto"
  | "mirror_auto";

export interface SyncJob {
  id: string;
  label: string;
  source: SyncSource;
  remotePath: string;
  mode: SyncMode;
  intervalMinutes: number;
  deleteEnabled: boolean;
  lastSync: number;
}

export type ConnectionMode = "local" | "zerotier" | "wireguard";

export interface PublicConfig {
  setupComplete: boolean;
  zerotierNetworkId: string;
  connectionMode: ConnectionMode;
  smb: SmbConfig;
  syncJobs: SyncJob[];
  remoteGalleryPath: string;
  suggestedGalleryPath: string;
  galleryEnabled: boolean;
  galleryColumns: number;
  fileNameScale: number;
  fileViewMode: "list" | "details";
  treeShowFiles: boolean;
  treeFontScale: number;
  themeColor: string;
  externalDocuments: boolean;
  externalSaveEnabled: boolean;
  externalSaveMode: "auto" | "button" | string;
  downloadDir: string;
  downloadAutoSave: boolean;
  downloadKeepStructure: boolean;
  systemInstallDir: string;
  systemInstalled: boolean;
  systemAutostart: boolean;
  systemStartMenu: boolean;
  systemDesktopShortcut: boolean;
  systemTray: boolean;
  systemNotifications: boolean;
  notifyDownloads: boolean;
  notifyUploads: boolean;
  notifySync: boolean;
  notifyErrors: boolean;
  wireguardConfigPath: string;
  wireguardEnabled: boolean;
  aiEnabled: boolean;
  aiAllowDownload: boolean;
  aiAllowModify: boolean;
  aiAllowUpload: boolean;
  aiAllowMkdir: boolean;
  aiAllowDelete: boolean;
  aiDenyPaths: string[];
  androidTreeUri: string | null;
  updateRepo: string;
  customRoots: boolean;
  planetConfigured: boolean;
  hasPassword: boolean;
  platform: "android" | "windows" | "linux" | string;
  android: boolean;
}

export interface SetupInput {
  zerotierNetworkId: string;
  connectionMode?: ConnectionMode;
  smb: SmbConfig;
  password: string;
  remoteGalleryPath?: string;
  systemInstallDir?: string;
  systemAutostart?: boolean;
  systemStartMenu?: boolean;
  systemDesktopShortcut?: boolean;
  systemTray?: boolean;
  systemNotifications?: boolean;
  notifyDownloads?: boolean;
  notifyUploads?: boolean;
  notifySync?: boolean;
  notifyErrors?: boolean;
  downloadDir?: string;
  downloadAutoSave?: boolean;
  downloadKeepStructure?: boolean;
}

export interface SettingsInput {
  smb: SmbConfig;
  password?: string;
  remoteGalleryPath?: string;
  galleryEnabled?: boolean;
  galleryColumns?: number;
  fileNameScale?: number;
  fileViewMode?: "list" | "details";
  treeShowFiles?: boolean;
  treeFontScale?: number;
  themeColor?: string;
  externalDocuments?: boolean;
  externalSaveEnabled?: boolean;
  externalSaveMode?: "auto" | "button";
  downloadDir?: string;
  downloadAutoSave?: boolean;
  downloadKeepStructure?: boolean;
  systemInstallDir?: string;
  systemAutostart?: boolean;
  systemStartMenu?: boolean;
  systemDesktopShortcut?: boolean;
  systemTray?: boolean;
  systemNotifications?: boolean;
  notifyDownloads?: boolean;
  notifyUploads?: boolean;
  notifySync?: boolean;
  notifyErrors?: boolean;
  wireguardConfigPath?: string;
  wireguardEnabled?: boolean;
  zerotierNetworkId?: string;
  connectionMode?: ConnectionMode;
  aiEnabled?: boolean;
  aiAllowDownload?: boolean;
  aiAllowModify?: boolean;
  aiAllowUpload?: boolean;
  aiAllowMkdir?: boolean;
  aiAllowDelete?: boolean;
  aiDenyPaths?: string[];
  updateRepo?: string;
}

export interface AiStatus {
  enabled: boolean;
  running: boolean;
  url: string;
  error: string;
}

export interface WireGuardStatus {
  configured: boolean;
  connected: boolean;
  configPath: string;
  message: string;
}

export interface ExternalFile {
  localPath: string;
  signature: string;
}

export interface ExternalEditSession {
  remotePath: string;
  localPath: string;
  name: string;
  signature: string;
}

export interface DocumentPreview {
  name: string;
  text: string;
}

export interface ArchiveEntry {
  index: number;
  name: string;
  size: number;
  isDir: boolean;
}

export interface TransferItem {
  id: string;
  kind: "upload" | "download" | string;
  name: string;
  total: number;
  transferred: number;
  status: "queued" | "active" | "done" | "error" | string;
  error: string;
}

export interface TransferSnapshot {
  items: TransferItem[];
}

export interface RuntimeInfo {
  platform: string;
  android: boolean;
  version: string;
}

export interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modified: number;
  extension: string;
}

export type FileKind =
  | "image"
  | "gif"
  | "video"
  | "text"
  | "pdf"
  | "document"
  | "archive"
  | "file";

export interface MediaRef {
  path: string;
  kind: FileKind;
}

export interface UploadResult {
  uploaded: number;
  failed: number;
  errors: string[];
}

export interface GalleryItem {
  uri: string;
  name: string;
  mime: string;
  size: number;
  dateModified: number;
  dateCreated: number;
  relativePath: string;
  bucket: string;
}

export interface GalleryList {
  items: GalleryItem[];
}

export interface ZtNetwork {
  id: string;
  name: string;
  status: string;
  assignedAddresses: string[];
}

export interface ZtStatus {
  available: boolean;
  serviceOnline: boolean;
  version: string | null;
  address: string | null;
  networks: ZtNetwork[];
  message: string;
  android: boolean;
}

export interface SyncProgress {
  phase: string;
  current: number;
  total: number;
  file: string;
  bytes: number;
  skipped: number;
  failed: number;
  message: string | null;
}

export interface PlanetInfo {
  configured: boolean;
  customRoots: boolean;
  size: number;
  message: string;
}

export interface UpdateInfo {
  available: boolean;
  currentVersion: string;
  latestVersion: string;
  notes: string;
  publishedAt: string | null;
  assetName: string | null;
  assetUrl: string | null;
  repo: string;
  platform: string;
  message: string;
}

export interface DeleteResult {
  deleted: number;
  failed: number;
  errors: string[];
}

export interface DownloadResult {
  downloaded: number;
  skipped: number;
  failed: number;
  bytes: number;
  destination: string;
  errors: string[];
}

export interface SyncSummary {
  uploaded: number;
  downloaded: number;
  deleted: number;
  skipped: number;
  failed: number;
  bytes: number;
  errors: string[];
}

export interface ViewerItem {
  url?: string;
  remotePath?: string;
  relativeDir?: string;
  downloadConflict?: "skip" | "rename";
  name: string;
  kind: FileKind;
}
