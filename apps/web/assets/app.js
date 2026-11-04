// === 0. 최상단 설정 변수: 동일 정책을 추후 config/client.example.yaml로부터 생성 ===
const SETTINGS = Object.freeze({
  historyLimit: 250,         // 각 노트에 영속화할 최대 Undo/Redo 기록 개수
  maxFolderDepth: 2,        // 루트부터 허용하는 폴더 깊이(폴더 최대 2개)
  idleAfterMs: 60_000,      // 입력이 없으면 활성 편집시간 측정 종료
  saveDebounceMs: 300,      // 저장 요청 지연, 너무 잦은 IndexedDB 작업 방지
  timerTickMs: 1000,        // 세션 활성시간 갱신 간격
  indentation: '  ',       // Markdown 블록 들여쓰기: 공백 두 칸
});
const WORDS = {
  ko: {folders:'디렉터리', newFolder:'+ 폴더', newNote:'+ 메모', search:'검색', trees:'Tree (연관 그래프는 서버 연결 후)', writing:'메모', focus:'집중 모드 (F6)', split:'2패널', undo:'실행취소', redo:'다시실행', export:'다운로드', import:'가져오기', sessions:'편집 세션 기록'},
  en: {folders:'Folders', newFolder:'+ Folder', newNote:'+ Note', search:'Search', trees:'Tree (graph: server planned)', writing:'Writer', focus:'Focus (F6)', split:'Split view', undo:'Undo', redo:'Redo', export:'Download', import:'Import', sessions:'Editing sessions'},
};
import { localStore } from './store.js';

// === 1. 내부 UI 상태: 서버 계정/동기화는 구현하지 않습니다 ===
const state = {notes:[], folders:[], selectedId:null, selectedFolder:'', locale:'ko', note:null, mode:'split', lastInputAt:0, activeSessionId:null, lastTimerTick:Date.now(), saveTimer:null, saving:Promise.resolve()};
const $ = (id) => document.getElementById(id);
const nowISO = () => new Date().toISOString();
const uid = () => crypto.randomUUID();
const safeFilename = (name) => name.normalize('NFC').trim().replace(/[\\/:*?"<>|\u0000-\u001f]/g,'_').slice(0,200) || 'Untitled';
const timeText = (milliseconds) => { const t=Math.floor(milliseconds/1000); return [Math.floor(t/3600),Math.floor(t/60)%60,t%60].map(x=>String(x).padStart(2,'0')).join(':'); };
const isSmallScreen = () => window.matchMedia('(max-width:760px)').matches;

// 서버 없는 단독 프로토타입에서 0-2 단계 경로만 유효하도록 검증합니다.
function parseFolder(raw) {
  const bits=raw.replaceAll('\\','/').split('/').filter(Boolean).map(x=>x.trim());
  if(bits.length>SETTINGS.maxFolderDepth || bits.some(x=>x==='.'||x==='..'||!x||x.includes(':')||x.length>70)) throw Error('폴더는 최대 2단계이고 상대 경로 기호를 사용할 수 없습니다.');
  return bits.join('/');
}
function snapshot(note) { return {title:note.title, content:note.content, format:note.format}; }
function remember(note) {
  note.undo ??=[];note.redo ??=[];
  note.undo.push(snapshot(note));
  if(note.undo.length>SETTINGS.historyLimit) note.undo.shift();
  note.redo=[];
}
function setLanguage(locale) {
  state.locale=locale;document.documentElement.lang=locale;
  document.querySelectorAll('[data-i18n]').forEach(node=>{node.textContent=WORDS[locale][node.dataset.i18n]||node.textContent;});
  $('search').placeholder=locale==='ko'?'제목 또는 본문':'Title or body';
  $('title').placeholder=locale==='ko'?'메모 제목':'Note title';
  $('editor').placeholder=locale==='ko'?'여기에 생각을 작성하세요. Markdown을 그대로 입력합니다.':'Write here. Plain Markdown text.';
  localStore.setMeta('locale',locale).catch(console.error);
}
function updateConnectivity() {
  $('connectivity').textContent=navigator.onLine?'LOCAL · ONLINE':'LOCAL · OFFLINE';
  // 브라우저는 Wi-Fi 종류/BT 및 배터리 정보를 안정적으로 제공하지 않습니다.
  $('wifi').textContent=navigator.onLine?'NET: ONLINE':'NET: OFFLINE';
  $('battery').textContent='BAT: --';$('bluetooth').textContent='BT: --';
}
function renderClock() { $('clock').textContent=new Date().toLocaleString(state.locale==='ko'?'ko-KR':'en-US'); }
function changeMode(mode) {state.mode=mode;document.body.className=`mode-${mode}`;}
function displayTree() {
  const tree=$('tree');tree.replaceChildren();
  const query=$('search').value.toLocaleLowerCase();
  const rows=state.notes.filter(n=>!query||(`${n.title} ${n.content}`).toLocaleLowerCase().includes(query));
  const folderPaths=[...new Set([...state.folders.map(x=>x.path),...rows.flatMap(n=>n.folder?n.folder.split('/').map((_,i,a)=>a.slice(0,i+1).join('/')):[])])].sort();
  const makeButton=(title,cls,indent,selected,handler)=>{
    const b=document.createElement('button');b.type='button';b.className=`${cls} tree-indent-${Math.min(indent,3)}`;b.textContent=title;b.setAttribute('aria-selected',String(selected));b.addEventListener('click',handler);tree.append(b);
  };
  makeButton('▸ /', 'folder',0,state.selectedFolder==='',()=>{state.selectedFolder='';$('folder-path').textContent='/';displayTree();});
  // 폴더와 노트를 계층 경로 순서대로 표시합니다(파일 선택은 UUID 기준).
  const byFolder=new Map();
  rows.forEach(note=>{const key=note.folder||'';if(!byFolder.has(key))byFolder.set(key,[]);byFolder.get(key).push(note);});
  function showNotes(folder,depth){for(const note of (byFolder.get(folder)||[]).sort((a,b)=>b.modifiedAt.localeCompare(a.modifiedAt))){
    makeButton('· '+note.title+'.'+note.format,'note',depth,note.id===state.selectedId,()=>selectNote(note.id));
  }}
  showNotes('',1);
  for(const folder of folderPaths){
    const depth=folder.split('/').length;
    makeButton('▸ '+folder.split('/').at(-1),'folder',depth,state.selectedFolder===folder,()=>{state.selectedFolder=folder;$('folder-path').textContent='/'+folder;displayTree();});
    showNotes(folder,Math.min(depth+1,3));
  }
  $('count').textContent=`${rows.length} notes`;
}
function renderSessions() {
  const node=$('sessions');node.replaceChildren();
  for(const s of (state.note?.sessions||[]).slice(-20).reverse()){
    const p=document.createElement('p');p.textContent=`${new Date(s.startedAt).toLocaleString()} - ${s.endedAt?new Date(s.endedAt).toLocaleString():'...'} (${timeText(s.activeMs||0)})`;node.append(p);
  }
}
function renderEditor() {
  const note=state.note;
  if(!note) {$('title').value='';$('editor').value='';$('save-status').textContent='NO NOTE';return;}
  $('title').value=note.title;$('editor').value=note.content;$('format').value=note.format;
  $('format-label').textContent='.'+note.format;
  $('created-at').textContent=new Date(note.createdAt).toLocaleString();
  $('timer').textContent=timeText(note.sessions.reduce((a,s)=>a+(s.activeMs||0),0));
  $('save-status').textContent='LOCAL SAVED';renderSessions();
}
async function persist(note) {
  // 같은 파일의 입력 순서를 보장하도록 직렬화합니다.
  const copy=structuredClone(note);
  state.saving=state.saving.catch(console.error).then(()=>localStore.saveNote(copy));
  try {await state.saving;$('save-status').textContent='LOCAL SAVED';}
  catch(e){$('save-status').textContent='SAVE FAILED';console.error(e);alert('로컬 저장이 실패했습니다. 공간/권한을 확인하세요.');}
}
function scheduleSave() {
  if(!state.note)return;
  $('save-status').textContent='SAVING...';clearTimeout(state.saveTimer);
  const target=state.note;
  state.saveTimer=setTimeout(()=>persist(target),SETTINGS.saveDebounceMs);
}
async function flushSave() {clearTimeout(state.saveTimer);if(state.note)await persist(state.note);}
function endSession() {
  const session=(state.note?.sessions||[]).find(s=>s.id===state.activeSessionId);
  if(session&&!session.endedAt){session.endedAt=nowISO();scheduleSave();}
  state.activeSessionId=null;
}
function editActivity(){
  const note=state.note;if(!note)return;
  const current=Date.now();
  if(state.activeSessionId && current-state.lastInputAt>SETTINGS.idleAfterMs)endSession();
  if(!state.activeSessionId){const session={id:uid(),startedAt:nowISO(),endedAt:null,activeMs:0};note.sessions.push(session);state.activeSessionId=session.id;}
  state.lastInputAt=current;state.lastTimerTick=current;renderSessions();
}
function applyEdit(field,value){
  const note=state.note;if(!note||note[field]===value)return;
  remember(note);note[field]=value;note.modifiedAt=nowISO();editActivity();scheduleSave();displayTree();
}
function restoreHistory(kind){
  const note=state.note;if(!note)return;
  const source=note[kind];if(!source?.length)return;
  const target=kind==='undo'?'redo':'undo';note[target].push(snapshot(note));if(note[target].length>SETTINGS.historyLimit)note[target].shift();
  Object.assign(note,source.pop());note.modifiedAt=nowISO();editActivity();scheduleSave();renderEditor();displayTree();
}
async function selectNote(id) {
  if(id===state.selectedId)return;
  endSession();await flushSave();
  state.selectedId=id;state.note=state.notes.find(n=>n.id===id)||null;
  state.activeSessionId=null;state.lastInputAt=0;renderEditor();displayTree();
  await localStore.setMeta('selectedId',id);
  if(isSmallScreen())changeMode('editor');
}
async function addNote(title, content='', format='md') {
  const sanitized=safeFilename(title);
  const exists=state.notes.some(n=>n.folder===state.selectedFolder&&n.title===sanitized&&n.format===format);
  if(exists){alert('같은 폴더에 같은 이름의 메모가 있습니다.');return;}
  const time=nowISO();
  const note={id:uid(),title:sanitized,folder:state.selectedFolder,format,content,createdAt:time,modifiedAt:time,sessions:[],undo:[],redo:[]};
  await localStore.saveNote(note);state.notes.push(note);await selectNote(note.id);
}
async function addFolder(raw) {
  const path=parseFolder(raw);if(!path)return;
  const bits=path.split('/');for(let i=1;i<=bits.length;i++){
    const part=bits.slice(0,i).join('/');if(!state.folders.some(f=>f.path===part)){await localStore.saveFolder(part);state.folders.push({path:part});}
  }
  state.selectedFolder=path;$('folder-path').textContent='/'+path;displayTree();
}
function downloadNote(){
  const note=state.note;if(!note)return;
  const blob=new Blob([note.content],{type:'text/plain;charset=utf-8'});
  const a=document.createElement('a');const url=URL.createObjectURL(blob);
  a.href=url;a.download=`${safeFilename(note.title)}.${note.format}`;a.click();setTimeout(()=>URL.revokeObjectURL(url),2500);
}
function indentSelection(outdent=false){
  const box=$('editor');const note=state.note;if(!note)return;
  const start=box.selectionStart,end=box.selectionEnd;
  const first=box.value.lastIndexOf('\n',start-1)+1;
  const selected=box.value.slice(first,end);
  const updated=selected.split('\n').map(line=>outdent?line.replace(/^( {1,2}|\t)/,''):SETTINGS.indentation+line).join('\n');
  box.setRangeText(updated,first,end,'select');applyEdit('content',box.value);
}
function updateTimer(){
  const current=Date.now();const session=(state.note?.sessions||[]).find(s=>s.id===state.activeSessionId);
  if(session){
    if(current-state.lastInputAt>SETTINGS.idleAfterMs||document.visibilityState==='hidden'){endSession();}
    else {const increment=Math.max(0,Math.min(current-state.lastTimerTick,SETTINGS.timerTickMs*2));session.activeMs+=increment;state.lastTimerTick=current;scheduleSave();}
  }
  if(state.note){$('timer').textContent=timeText(state.note.sessions.reduce((a,s)=>a+(s.activeMs||0),0));}
}
async function bootstrap(){
  const meta=await localStore.getMeta('locale');state.notes=await localStore.notes();state.folders=await localStore.folders();
  state.locale=meta?.value==='en'?'en':'ko';setLanguage(state.locale);
  const selected=await localStore.getMeta('selectedId');
  const initial=state.notes.find(n=>n.id===selected?.value)||state.notes[0];
  if(initial){state.selectedId=initial.id;state.note=initial;}
  renderEditor();displayTree();renderClock();updateConnectivity();
  $('add-note').onclick=async()=>{const value=prompt(state.locale==='ko'?'새 메모 제목':'Note title','Untitled');if(value!==null&&value.trim())await addNote(value);};
  $('add-folder').onclick=async()=>{const value=prompt('폴더명 (예: Work/Project)',state.selectedFolder?state.selectedFolder+'/':'');if(value!==null)try{await addFolder(value);}catch(e){alert(e.message);}};
  $('title').oninput=e=>applyEdit('title',safeFilename(e.target.value));
  $('editor').oninput=e=>applyEdit('content',e.target.value);
  $('format').onchange=e=>{const note=state.note;if(note){remember(note);note.format=e.target.value;note.modifiedAt=nowISO();scheduleSave();renderEditor();displayTree();}};
  $('search').oninput=displayTree;
  $('language-btn').onclick=()=>setLanguage(state.locale==='ko'?'en':'ko');
  $('toggle-btn').onclick=()=>changeMode(state.mode==='editor'?'organizer':'editor');
  $('split-btn').onclick=()=>changeMode('split');
  $('undo-btn').onclick=()=>restoreHistory('undo');$('redo-btn').onclick=()=>restoreHistory('redo');
  $('indent-btn').onclick=()=>indentSelection();$('outdent-btn').onclick=()=>indentSelection(true);
  $('export-btn').onclick=downloadNote;
  $('import-btn').onclick=()=>$('import-file').click();
  $('import-file').onchange=async(e)=>{const file=e.target.files?.[0];if(!file)return;const format=file.name.toLowerCase().endsWith('.txt')?'txt':'md';await addNote(file.name.replace(/\.(md|txt)$/i,''),await file.text(),format);e.target.value='';};
  $('editor').onkeydown=e=>{
    if(e.key==='Tab'){e.preventDefault();indentSelection(e.shiftKey);}
    const mod=e.ctrlKey||e.metaKey;
    if(mod&&e.key.toLowerCase()==='z'){e.preventDefault();restoreHistory(e.shiftKey?'redo':'undo');}
    else if(mod&&e.key.toLowerCase()==='y'){e.preventDefault();restoreHistory('redo');}
  };
  document.addEventListener('keydown',e=>{if(e.key==='F6'){e.preventDefault();changeMode(state.mode==='editor'?'organizer':'editor');}});
  document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='hidden'){endSession();flushSave();}});
  window.addEventListener('beforeunload',()=>{endSession();/* debounce save has often run; native adapter will use a durable journal */});
  window.addEventListener('online',updateConnectivity);window.addEventListener('offline',updateConnectivity);
  setInterval(updateTimer,SETTINGS.timerTickMs);setInterval(renderClock,10_000);
  if('serviceWorker' in navigator)navigator.serviceWorker.register('./sw.js').catch(console.error);
}
bootstrap().catch(e=>{console.error(e);$('save-status').textContent='STORAGE UNAVAILABLE';alert('브라우저 IndexedDB를 사용할 수 없습니다. 보안 모드/저장공간을 확인하세요.');});
