/* Local design prototype. No requests, storage, AI service, or host integration. */
const design = ['a','b','c'].includes(new URLSearchParams(location.search).get('design')) ? new URLSearchParams(location.search).get('design') : 'a';
const names = {a:['01 / COMPACT TOOLBAR','Quiet companion'],b:['02 / TWO-TIER NAVIGATION','Pearl workspace'],c:['03 / CONVERSATION-LED','A thread to return to']};
const providers = [{name:'Claude',model:'Sonnet',color:'#fab387'},{name:'Codex',model:'GPT',color:'#f9e2af'},{name:'Gemini',model:'Pro',color:'#a6e3a1'}];
const initial = [
 {mine:true,text:'I have an hour before bed. Help me wind down without my phone.'},
 {mine:false,html:'<p>Let’s keep it gentle. Pick a little rhythm for the hour:</p><ol><li><strong>10 minutes:</strong> charge your phone across the room.</li><li><strong>30 minutes:</strong> a book, a shower, or quiet music.</li><li><strong>20 minutes:</strong> get ready for tomorrow, then rest.</li></ol><p>Is it a busy mind that usually keeps you awake?</p>'},
 {mine:true,text:'Yes. I keep remembering things I need to do tomorrow.'},
 {mine:false,html:'<p>Try a tiny “tomorrow list” on paper. Write each thing in a few words, then choose <strong>one to start with</strong> in the morning.</p><p>The list can hold those thoughts for tonight.</p>'}
];
let turns = structuredClone(initial), provider = 0, view = 'chat', draft = '', baseline = false;
const panel = document.querySelector('.panel'), content = document.querySelector('#content'), footer = document.querySelector('#composer-region');
panel.classList.add('design-'+design);
document.title = 'Tohru · '+names[design][1];
document.querySelector('#direction-label').textContent=names[design][0];
document.querySelector('#direction-title').textContent=names[design][1];
const icon = name => `<span aria-hidden="true" class="icon" style="--icon:url(assets/${name}.svg)"></span>`;
const iconButton = (name,label,action) => `<button class="icon-button" data-action="${action}" aria-label="${label}" title="${label}">${icon(name)}</button>`;
const identity = `<div class="identity"><img class="mascot" src="assets/mascot.png" alt=""><div class="brand">Tohru<span class="sub-brand">Your desktop assistant</span></div></div>`;
function providerButton(){ const p = providers[provider];return `<button type="button" class="provider-button" data-action="providers" aria-label="Choose provider and model: ${p.name}, ${p.model}" aria-expanded="${view==='providers'}"><span class="provider-dot" style="background:${p.color}"></span>${p.name} <span>· ${p.model}</span><span class="chevron" aria-hidden="true">⌄</span></button>`; }
function renderHeader(){
 const history = iconButton('history',view==='history'?'Back to chat':'History','history'), fresh=iconButton('plus','New chat','new');
 const title=turns.length?'A calmer evening':'New conversation';
 let html;
 if(design==='a') html=`<div class="header-a">${identity}<div class="spacer"></div>${history}${fresh}</div>`;
 if(design==='b') html=`<div class="header-b"><div class="topline">${identity}<div class="spacer"></div><button class="new-button" data-action="new">${icon('plus')}New chat</button></div><nav class="tabs" aria-label="Assistant views"><button class="tab" aria-pressed="${view==='chat'}" data-action="chat">Chat</button><button class="tab" aria-pressed="${view==='history'}" data-action="history">History</button>${providerButton()}</nav></div>`;
 if(design==='c') html=`<div class="header-c"><div class="brandline"><img class="mascot" src="assets/mascot.png" alt=""><span class="brand">Tohru</span><span class="subtle">/ Assistant</span><div class="spacer"></div>${history}${fresh}</div><div class="thread-heading"><div class="eyebrow">${view==='history'?'YOUR CONVERSATIONS':view==='providers'?'CONVERSATION SETTINGS':'CONVERSATION'}</div><h2>${view==='history'?'History':view==='providers'?'Choose an assistant':title}</h2><p>${view==='chat'?'A little space to think things through.':'Your conversation stays right here.'}</p></div></div>`;
 document.querySelector('#app-header').innerHTML=html;
}
function renderFooter(){
 footer.hidden=view!=='chat';
 footer.innerHTML=`<div class="composer-wrap">${design==='c'?'<div class="reply-label">'+(turns.length?'Continue the conversation':'Start a conversation')+'</div>':''}<form class="composer" aria-label="Message composer"><textarea aria-label="Message Tohru" placeholder="${turns.length?'Reply to Tohru…':'Ask Tohru anything…'}" rows="2"></textarea><div class="composer-bottom">${design==='b'?'<span class="hint-inline">Enter to send · Shift + Enter for a line</span>':providerButton()}<div class="spacer"></div><button class="send" type="submit" aria-label="Send message" disabled>${icon('send')}</button></div></form>${design==='a'?'<p class="composer-hint">Enter to send · Shift + Enter for a new line</p>':''}</div>`;
 const input=footer.querySelector('textarea');input.value=draft;footer.querySelector('.send').disabled=!draft.trim();
 input.addEventListener('input',()=>{draft=input.value;footer.querySelector('.send').disabled=!draft.trim();});
 input.addEventListener('keydown',event=>{if(event.key==='Enter'&&!event.shiftKey){event.preventDefault();send();}});
 footer.querySelector('form').addEventListener('submit',event=>{event.preventDefault();send();});
}
function messageHtml(turn,index){return `<article class="message ${turn.mine?'user':'assistant'}" aria-label="${turn.mine?'You':'Tohru'}">${turn.mine?'':'<div class="speaker">Tohru</div>'}${turn.mine?'<p>'+escapeHtml(turn.text)+'</p>':turn.html||'<p>'+escapeHtml(turn.text)+'</p>'}${!turn.mine&&index===turns.length-1?`<div class="answer-actions">${iconButton('copy','Copy answer','copy')}${iconButton('retry','Retry answer','retry')}</div>`:''}</article>`;}
function renderContent(){
 content.setAttribute('aria-label',view==='chat'?'Conversation':view==='history'?'Conversation history':'Provider selection');
 if(view==='chat')content.innerHTML=turns.length?'<div class="day-label">Today</div>'+turns.map(messageHtml).join(''):`<div class="empty-state"><img src="assets/mascot.png" alt=""><h2>What’s on your mind?</h2><p>A question, a loose thought, or a small thing you’d like a hand with.</p><button class="suggestion" data-action="suggest">Help me make space for a quiet evening ↗</button></div>`;
 if(view==='providers')content.innerHTML=`<div class="view-heading"><button class="back-button" data-action="chat" aria-label="Back to conversation">←</button><h2>Provider & model</h2></div><p class="view-intro">Choose who answers in this conversation.<br>These are sample options for this design study.</p>${providers.map((p,i)=>`<button class="provider-option" data-provider="${i}" aria-pressed="${i===provider}"><span class="provider-dot" style="background:${p.color}"></span><span class="option-details"><strong>${p.name}</strong><small>${p.model}</small></span><span class="option-check">${i===provider?'Selected':'Choose'}</span></button>`).join('')}`;
 if(view==='history')content.innerHTML=`<div class="view-heading"><button class="back-button" data-action="chat" aria-label="Back to conversation">←</button><h2>History</h2><div class="spacer"></div><button class="new-button" data-action="new">${icon('plus')}New chat</button></div><input type="search" class="search-box" aria-label="Search conversations" placeholder="Search conversations"><div id="history-results"></div>`;
 if(view==='history'){renderHistory('');content.querySelector('input').addEventListener('input',e=>renderHistory(e.target.value));}
 content.scrollTop=0;
}
const histories=[['Today','A calmer evening','18:42 · Claude'],['Today','A simple dinner from the fridge','17:15 · Claude'],['Yesterday','What to pack for the weekend','Yesterday · Codex'],['This week','A few books for rainy days','Tuesday · Claude']];
function renderHistory(query){let group='';const rows=histories.filter(row=>row[1].toLowerCase().includes(query.toLowerCase()));document.querySelector('#history-results').innerHTML=rows.length?rows.map(row=>{let heading=row[0]!==group?`<div class="history-group">${row[0]}</div>`:'';group=row[0];return heading+`<button class="history-item" data-action="open-history"><span class="option-details"><strong>${row[1]}</strong><small>${row[2]}</small></span><span aria-hidden="true">↗</span></button>`;}).join(''):'<p class="view-intro">No conversations found.</p>';}
function render(){renderHeader();renderContent();renderFooter();measure();}
function setView(next){view=next;render();if(next==='history')content.querySelector('input').focus();else if(next==='providers')content.querySelector('.provider-option').focus();}
function send(){if(!draft.trim())return;turns.push({mine:true,text:draft.trim()},{mine:false,text:'This is a local demo reply. Your message stays in this page; no AI service was contacted. In the real conversation, Tohru’s answer would appear here.'});draft='';view='chat';render();content.scrollTop=content.scrollHeight;footer.querySelector('textarea').focus();}
function notify(text){const toast=document.querySelector('.toast');toast.textContent=text;toast.classList.add('visible');setTimeout(()=>toast.classList.remove('visible'),2200);}
function escapeHtml(text){return text.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
document.addEventListener('click',async event=>{
 const option=event.target.closest('[data-provider]');if(option){provider=Number(option.dataset.provider);setView('chat');footer.querySelector('textarea').focus();return;}
 const target=event.target.closest('[data-action]');if(!target)return;const action=target.dataset.action;
 if(action==='new'){turns=[];draft='';setView('chat');footer.querySelector('textarea').focus();}
 if(action==='chat')setView('chat');if(action==='history')setView(view==='history'?'chat':'history');if(action==='providers')setView('providers');
 if(action==='open-history'){turns=structuredClone(initial);draft='';setView('chat');notify('Sample conversation opened');}
 if(action==='suggest'){draft=initial[0].text;renderFooter();footer.querySelector('textarea').focus();}
 if(action==='copy'){const answer=[...turns].reverse().find(t=>!t.mine);const el=document.createElement('div');el.innerHTML=answer.html||'';try{await navigator.clipboard.writeText(answer.text||el.textContent);notify('Answer copied');}catch{notify('Clipboard unavailable in this preview');}}
 if(action==='retry')notify('Demo only · no request sent');
});
document.addEventListener('keydown',event=>{if(event.key==='Escape'&&view!=='chat'){setView('chat');footer.querySelector('textarea').focus();}});
// Explicit keys keep the nested reading region usable in embedded browsers too.
content.addEventListener('keydown',event=>{
 if(event.target!==content)return;
 const steps={ArrowDown:40,ArrowUp:-40,PageDown:content.clientHeight*.85,PageUp:-content.clientHeight*.85};
 if(event.key in steps){event.preventDefault();content.scrollTop+=steps[event.key];}
 if(event.key==='Home'||event.key==='End'){event.preventDefault();content.scrollTop=event.key==='Home'?0:content.scrollHeight;}
});
document.querySelector('#sample').onclick=()=>{turns=structuredClone(initial);draft='';setView('chat');};
document.querySelector('#long').onclick=()=>{turns=Array.from({length:6},()=>structuredClone(initial)).flat();draft='';setView('chat');content.scrollTop=content.scrollHeight;};
document.querySelector('#baseline').onclick=()=>{baseline=!baseline;document.querySelector('.baseline').classList.toggle('on',baseline);document.querySelector('#baseline').setAttribute('aria-pressed',baseline);};
function measure(){const r=panel.getBoundingClientRect();document.querySelector('#dimensions').textContent=`${r.width} × ${r.height} logical px\nleft ${r.x} · top ${r.y}\nright ${innerWidth-r.right} · bottom ${innerHeight-r.bottom}`;}
window.addEventListener('resize',measure);
render();
