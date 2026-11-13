// === 프로젝트 (ADR-0005: 폴더 위 단계) — 화면·동기화 공통 규칙 ===
// - 기본 프로젝트 = ID '' (메모의 projectId가 없거나 null)
// - 이름 규칙: 폴더 한 칸과 같음(paths.json) + '/'·'@' 시작 금지, 같은 이름(대소문자 무시) 금지
// - 웹은 프로젝트 목록을 meta 'projects'에, 프로젝트별 빈 폴더는 folders 저장소에 '<프로젝트ID>::<경로>'로 둠
//   (':'는 폴더 이름에 쓸 수 없는 문자라 기존 폴더와 겹치지 않음 → IndexedDB 버전·형식 그대로)
import { FolderError, parseFolder } from './paths';
import type { Note } from './types';

export const DEFAULT_PROJECT = '';
export const PROJECTS_META_KEY = 'projects';
export const CURRENT_PROJECT_META_KEY = 'currentProject';
const FOLDER_KEY_SEPARATOR = '::';

export interface ProjectSync {
  /** 서버에 반영된 적 있음 */
  synced: boolean;
  /** 올리지 않은 변경 (만들기·이름 바꾸기) */
  dirty: boolean;
  /** 지우기를 기다리는 중 (서버가 거절하면 되돌림) */
  deleting?: boolean;
  error?: string | null;
}

export interface LocalProject {
  id: string;
  name: string;
  createdAt: string;
  modifiedAt: string;
  sync: ProjectSync;
}

export const projectOf = (note: Pick<Note, 'projectId'>): string => note.projectId ?? DEFAULT_PROJECT;
export const inProject = (note: Pick<Note, 'projectId'>, project: string): boolean => projectOf(note) === project;

/** 이름 검사·정리. 문제가 있으면 FolderError */
export function normalizeProjectName(raw: string, existing: LocalProject[], selfId?: string): string {
  const name = raw.trim();
  if (!name) throw new FolderError('invalid', '프로젝트 이름을 입력하세요.');
  if (name.includes('/') || name.includes('\\')) throw new FolderError('invalid', '프로젝트 이름에는 / 를 쓸 수 없습니다.');
  if (name.startsWith('@')) throw new FolderError('invalid', '프로젝트 이름은 @로 시작할 수 없습니다.');
  const clean = parseFolder(name, 1);
  const taken = existing.some(
    (p) => p.id !== selfId && !p.sync.deleting && p.name.toLocaleLowerCase() === clean.toLocaleLowerCase(),
  );
  if (taken) throw new FolderError('invalid', '같은 이름의 프로젝트가 있습니다.');
  return clean;
}

/** folders 저장소 키 */
export function folderKey(project: string, path: string): string {
  return project ? `${project}${FOLDER_KEY_SEPARATOR}${path}` : path;
}

export function parseFolderKey(key: string): { project: string; path: string } {
  const at = key.indexOf(FOLDER_KEY_SEPARATOR);
  return at < 0 ? { project: DEFAULT_PROJECT, path: key } : { project: key.slice(0, at), path: key.slice(at + 2) };
}
