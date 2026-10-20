/* 仅供设计验证：不调用 MCP，不使用真实项目内容。 */
const $ = selector => document.querySelector(selector);
const storageKey = 'turndock-design-v1';
const tasks = [
  {id:'demo-001',client:'Codex',project:'TurnDock / 设计',kind:'设计确认',time:'8 分钟',title:'先确认第一版的实现范围',description:'产品方向和交互流程已经整理好了。第一版先把人和 AI 之间的一次确认做好，你希望按哪个范围推进？',noteTitle:'已经确定',note:'本地优先、多个项目独立等待、回复前不自动继续。暂不加入云同步、代码搜索和远程机器人。',mode:'single',question:'你希望从哪里开始？',options:[['core','先做可靠的回复闭环','接收请求、保留草稿、明确回复，确认每一步都能追踪。'],['inbox','把多项目收件箱一起做好','在回复闭环基础上，加入队列、来源和聚焦视图。']],context:'这是一条预设的设计确认。正式产品会在此展示调用方提供的上下文；不会用模型自动改写你的选择。',state:'waiting',text:'',selected:[],attachments:[]},
  {id:'demo-002',client:'Claude Code',project:'Atlas / 导出',kind:'范围选择',time:'23 分钟',title:'这次导出包含哪些内容？',description:'报告与附件已整理完成。请选择需要包含的内容，可以选择多项，再补充命名或格式要求。',noteTitle:'导出边界',note:'本次只生成本地文件，不上传云端，也不覆盖已有报告。文件名冲突时会另存新文件。',mode:'multiple',question:'请选择导出内容',options:[['report','报告正文','包含章节目录与引用。'],['charts','图表与数据','图片与 CSV 数据一起打包。'],['notes','附上方法说明','便于之后复查处理过程。']],context:'演示项目 Atlas。多选仅适用于本条请求，不影响其他任务的选择和草稿。',state:'waiting',text:'',selected:[],attachments:[]},
  {id:'demo-003',client:'Cursor',project:'Notes / 整理',kind:'补充说明',time:'1 小时',title:'这两组笔记需要合并吗？',description:'我找到了两组主题接近的笔记，但它们采用了不同的目录结构。请说明你希望保留哪一种，或继续分开整理。',noteTitle:'连接已断开',note:'这条演示请求的调用方已经退出。你仍然可以查看和复制草稿，但不能把回复发给一个新的会话。',mode:'none',question:'',options:[],context:'断线不等于用户同意。重新连接后，必须由新请求明确关联，才能复用原草稿。',state:'disconnected',text:'先保留两组目录，不要直接合并。',selected:[],attachments:[]}
];
let active = tasks[0].id, filter = 'pending', toastTimer, modalAction, modalTrigger;
let saveAvailable = true;
try {
  const saved = JSON.parse(localStorage.getItem(storageKey) || '{}');
  for (const task of tasks) {
    const old = saved.tasks?.[task.id];
    if (old) {
      if(typeof old.text === 'string') task.text = old.text;
      if(Array.isArray(old.selected)) task.selected = old.selected.filter(id => task.options.some(option=>option[0]===id));
      if(['waiting','disconnected','answered','declined'].includes(old.state)) task.state = old.state;
      if(typeof old.result === 'string') task.result = old.result;
    }
  }
  if(saved.theme==='dark') document.documentElement.dataset.theme='dark';
} catch { saveAvailable=false; }
const current = () => tasks.find(task=>task.id===active);
const isDone = task => ['answered','declined'].includes(task.state);
function persist() {
  try {
    localStorage.setItem(storageKey,JSON.stringify({theme:document.documentElement.dataset.theme||'light',tasks:Object.fromEntries(tasks.map(t=>[t.id,{text:t.text,selected:t.selected,state:t.state,result:t.result}]))}));
    saveAvailable=true;
  } catch {saveAvailable=false;}
  $('#save-state').textContent=saveAvailable?'文字与选择已保存在此浏览器':'保存失败，请先复制草稿';
}
function toast(message) { $('#toast').textContent=message;$('#toast').classList.add('visible');clearTimeout(toastTimer);toastTimer=setTimeout(()=>$('#toast').classList.remove('visible'),4200); }
function element(tag,className,text) {const el=document.createElement(tag);if(className)el.className=className;if(text!==undefined)el.textContent=text;return el;}
function visibleTasks() {const term=$('#search').value.toLowerCase();return tasks.filter(t=>(filter==='done'?isDone(t):!isDone(t)) && (t.title+t.project+t.client).toLowerCase().includes(term));}
function renderList() {
  $('#waiting-count').textContent=tasks.filter(t=>t.state==='waiting').length;
  $('#filter-pending').setAttribute('aria-pressed',String(filter==='pending'));$('#filter-done').setAttribute('aria-pressed',String(filter==='done'));
  const list=$('#task-list');list.replaceChildren();
  for (const task of visibleTasks()) {
    const button=element('button','task');button.dataset.task=task.id;button.setAttribute('aria-current',String(task.id===active));
    const meta=element('span','task-meta');meta.append(element('span','',task.client),element('span','',task.time));
    const bottom=element('span','task-bottom');bottom.append(element('span','',task.project));
    bottom.append(element('span',task.text?'draft-tag':'',isDone(task)?'已处理':task.state==='disconnected'?'已断开':task.text?'有草稿':'待回复'));
    button.append(meta,element('span','task-title',task.title),bottom);
    button.addEventListener('click',()=>{active=task.id;$('#app').classList.remove('show-list');renderDetail();renderList();$('#request-title').focus();});list.append(button);
  }
  if(!list.children.length)list.append(element('p','list-empty',$('#search').value?'没有匹配的请求。':'这里暂时没有请求。'));
}
function updateSend() {const t=current();$('#send').disabled=!t||t.state!=='waiting'||(!t.text.trim()&&!t.selected.length&&!t.attachments.length);}
function renderAttachments() {
  const t=current(), list=$('#attachments');list.replaceChildren();if(!t)return;
  for(const file of t.attachments){const item=element('div','attachment');const image=element('img');image.src=file.url;image.alt='';const label=element('span','',file.name);const remove=element('button','','×');remove.setAttribute('aria-label','移除 '+file.name);remove.disabled=t.state!=='waiting';remove.addEventListener('click',()=>{t.attachments=t.attachments.filter(f=>f.id!==file.id);URL.revokeObjectURL(file.url);renderAttachments();updateSend();toast('已移除所选图片');});item.append(image,label,remove);list.append(item);}
}
function renderDetail() {
  const t=current();const empty=!t;
  for(const selector of ['.context-bar','#reading','.composer'])$(selector).hidden=empty;
  $('#empty').hidden=!empty;if(empty)return;
  $('#client').textContent=t.client;$('#project').textContent=t.project;
  $('#connection').textContent=t.state==='disconnected'?'模拟连接已断开':isDone(t)?'演示已处理':'模拟等待中';$('#connection').classList.toggle('offline',t.state==='disconnected');
  $('#request-kind').textContent=t.kind;$('#elapsed').textContent='等待 '+t.time;$('#request-title').textContent=t.title;$('#request-description').textContent=t.description;$('#note-title').textContent=t.noteTitle;$('#note-text').textContent=t.note;
  $('#details-text').textContent=t.context;$('#request-id').textContent='演示请求 '+t.id;$('#details').open=false;
  $('#options').hidden=!t.options.length;$('#option-legend').replaceChildren(document.createTextNode(t.question),element('span','',t.mode==='single'?'单选':'多选'));$('#option-list').replaceChildren();
  for(const [id,title,description] of t.options){const label=element('label','option'),input=element('input'),body=element('span');input.type=t.mode==='single'?'radio':'checkbox';input.name='selection';input.value=id;input.checked=t.selected.includes(id);input.disabled=t.state!=='waiting';input.addEventListener('change',()=>{if(t.mode==='single')t.selected=[id];else t.selected=input.checked?[...t.selected,id]:t.selected.filter(x=>x!==id);persist();updateSend();});body.append(element('strong','',title),element('small','',description));label.append(input,body);$('#option-list').append(label);}
  $('#reply').value=t.text;$('#reply').readOnly=t.state!=='waiting';$('#attach').disabled=t.state!=='waiting';$('#later').disabled=t.state!=='waiting';$('#decline').disabled=t.state!=='waiting';$('#send').textContent='发送回复 ↵';
  $('#save-state').textContent=isDone(t)?'演示记录':!saveAvailable?'浏览器存储不可用，请复制草稿':'草稿仅保存在此浏览器';
  const notice=$('#state-notice');notice.hidden=t.state==='waiting';notice.textContent=t.state==='disconnected'?'调用方已断开。草稿可查看和复制，本原型不会重新投递。':t.state==='declined'?'演示：你已明确拒绝这条请求。没有向真实 AI 发送内容。':t.state==='answered'?'演示回复已记录（没有真实投递）。\n'+(t.result||''):'';
  renderAttachments();updateSend();$('#reading').scrollTop=0;
}
function changeFilter(next) {filter=next;const visible=visibleTasks();active=visible.some(t=>t.id===active)?active:visible[0]?.id;renderList();renderDetail();}
function showDialog(title,body,confirm,action){modalTrigger=document.activeElement;$('#dialog-title').textContent=title;$('#dialog-body').textContent=body;$('#dialog-confirm').textContent=confirm;modalAction=action;$('#dialog').returnValue='';$('#dialog').showModal();}
$('#dialog').addEventListener('close',()=>{const action=modalAction;modalAction=null;if($('#dialog').returnValue==='confirm')action?.();modalTrigger?.focus();});
$('#reply').addEventListener('input',()=>{current().text=$('#reply').value;persist();renderList();updateSend();});
$('#reply').addEventListener('keydown',event=>{if((event.metaKey||event.ctrlKey)&&event.key==='Enter'&&!event.isComposing&&event.keyCode!==229){event.preventDefault();if(!$('#send').disabled)$('#send').click();}});
$('#filter-pending').addEventListener('click',()=>changeFilter('pending'));$('#filter-done').addEventListener('click',()=>changeFilter('done'));$('#show-pending').addEventListener('click',()=>changeFilter('pending'));
$('#search').addEventListener('input',renderList);
$('#later').addEventListener('click',()=>{persist();toast(saveAvailable?'草稿已保留；请求仍在等待，不会自动继续。':'草稿未能保存，请先复制内容。');});
$('#send').addEventListener('click',()=>{const t=current();if(t.state!=='waiting'||$('#send').disabled)return;t.state='answered';t.result=[...t.selected.map(id=>t.options.find(o=>o[0]===id)[1]),t.text,t.attachments.length?`图片 ${t.attachments.length} 张（仅本次页面会话）`:''].filter(Boolean).join('\n');persist();renderList();renderDetail();toast('演示回复已记录；没有向真实 AI 发送内容。');});
$('#decline').addEventListener('click',()=>{const t=current();showDialog('明确拒绝这条请求？','拒绝与稍后处理不同。正式产品会向调用方返回明确拒绝。本原型只记录演示状态。','确认拒绝',()=>{t.state='declined';persist();renderList();renderDetail();toast('已记录拒绝；没有发送真实消息。');});});
$('#about').addEventListener('click',()=>showDialog('回合 · 交互原型','这里展示多项目请求、独立草稿、选择、图片、主题与断线状态。没有真实 MCP 或后端。\n\n文字和选择保存在此浏览器；图片只在本次页面会话保留。请勿输入敏感内容。\n\n重置只清除此原型的演示记录，不影响寸止。','重置演示',()=>{try{localStorage.removeItem(storageKey);location.reload();}catch{toast('无法访问浏览器存储。');}}));
function syncTheme(){const dark=document.documentElement.dataset.theme==='dark';$('#theme').textContent=dark?'浅色':'深色';$('#theme').setAttribute('aria-label',dark?'切换为浅色主题':'切换为深色主题');}
$('#theme').addEventListener('click',()=>{document.documentElement.dataset.theme=document.documentElement.dataset.theme==='dark'?'light':'dark';syncTheme();persist();});
$('#focus-mode').addEventListener('click',()=>{const enabled=$('#app').classList.toggle('focused');$('#focus-mode').setAttribute('aria-pressed',String(enabled));$('#focus-mode').textContent=enabled?'退出聚焦':'聚焦';$('#app').classList.remove('show-list');});
$('#back').addEventListener('click',()=>{$('#app').classList.add('show-list');$('#filter-pending').focus();});
$('#attach').addEventListener('click',()=>$('#file').click());
$('#file').addEventListener('change',async()=>{
  const t=current();if(t.state!=='waiting')return;
  for(const file of $('#file').files){if(t.attachments.length>=4){toast('演示最多添加 4 张图片。');break;}if(!['image/jpeg','image/png','image/webp'].includes(file.type)||file.size>10*1024*1024){toast('请选择 10MiB 以内的 PNG、JPEG 或 WebP。');continue;}t.attachments.push({id:crypto.randomUUID(),name:file.name,url:URL.createObjectURL(file)});}
  $('#file').value='';renderAttachments();updateSend();toast('图片仅在此页面预览，不上传；刷新后不保留。');
});
syncTheme();renderList();renderDetail();
