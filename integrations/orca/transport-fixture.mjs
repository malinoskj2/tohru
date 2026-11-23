// This file is bundled with the patched Orca source by verify-transport.py.
import { createServer } from 'node:net'
import { createInterface } from 'node:readline'
import { BOT_VOICE_METHODS } from './src/main/runtime/rpc/methods/bots-voice'
import { BOT_HANDLERS } from './src/cli/bots/bot'
import { desktopVoiceOwnerTurn, deliverDesktopVoiceContinuation, rememberDesktopVoiceDispatch } from './src/main/bots/desktop-voice-continuations'
import { desktopVoiceTasks } from './src/main/bots/desktop-voice-task-delivery'

import { OrchestrationRequestEvidence } from './src/cli/orchestration/identity/orchestration-compatibility-envelope'
import { needsOrchestrationCallerResolution } from './src/main/runtime/rpc/orchestration/callers/orchestration-session-caller'

const chat = '12345678-1234-4234-8234-123456789abc'
let request = 'orca:voice-request'
let promptText = '[tohru-desktop:voice]\nCreate a workspace'
let from
function prompt() {
  return {itemId: from ? 'worker-mail' : request, sequence: 1, revision: 1, observedAt: 1,
    turnScope: {kind:'thread'}, body: {kind:'message', role:'user', blocks:[{type:'text',text:promptText}], ...(from ? {from} : {})}}
}
const runtime = {
  bots: { list: () => [{ id: 'tohru', chat }], assertEnabled() {} },
  host: { journalReads: async () => ({ promptItem: async () => prompt() }) },
  getOrchestrationDb: () => ({ getMessageById: () => ({type:'worker_done'}),
    getDispatchContextById: () => ({creator_orca_session_id:chat}),
    db: {prepare: () => ({get: () => ({dispatch_id:'worker-dispatch',task_id:'worker-task',
      sender_snapshot: JSON.stringify({title:'Workspace creation',agent:'codex',hostLabel:null})})})} })
}

async function rpc(method, params, options = {}) {
  const entry = BOT_VOICE_METHODS.find((item) => item.name === method)
  if (!entry) throw new Error(`Unknown test method ${method}`)
  const parsed = entry.params.parse(params)
  const producer = method === 'bots.voice.task'
  const evidence = new OrchestrationRequestEvidence(producer
    ? { ORCA_AGENT_SESSION_ID: options.caller ?? chat } : {})
  const envelope = evidence.envelopeFor(method, {remote: !!options.paired, mutation: true, requestId: undefined})
  const request = {method, params, ...envelope}
  // Replace only host session-record lookup; actual CLI evidence and server admission choose the caller.
  const context = { runtime, caller: {kind: 'local-cli'},
    ...(needsOrchestrationCallerResolution(request, undefined)
      ? { orchestrationCaller: {orcaSessionId: envelope.orchestrationCompatibilityEvidence.agentSessionId} } : {}),
    ...(options.paired ? {pairedDeviceId: 'remote'} : {}) }
  return { result: await entry.handler(parsed, context) }
}

async function handle(message) {
  if (message.op === 'rpc') return rpc(message.method, message.params, message)
  if (message.op === 'cli') {
    const flags = new Map()
    for (let i = 2; i < message.args.length; i += 2) {
      const key = message.args[i].replace(/^--/, '')
      if (key !== 'json') flags.set(key, message.args[i+1])
    }
    const command = message.args.slice(0, 2).join(' ')
    let answer
    const client = {call: async (method, params) => { answer = await rpc(method, params); return answer }}
    await BOT_HANDLERS[command]({ flags, client, json: true, cwd: process.cwd() })
    return {ok:true, ...answer}
  }
  if (message.op === 'new-turn') {
    desktopVoiceOwnerTurn(chat)
    request = message.request
    promptText = message.text ?? '[tohru-desktop:voice]\nAnother question'
    from = undefined
    return {ok:true}
  }
  if (message.op === 'worker-start') {
    await rememberDesktopVoiceDispatch(runtime, 'worker-dispatch')
    return {ok:true}
  }
  if (message.op === 'worker-done') {
    from = {kind:'agent',senders:[],orchestration:{message:'mail-notice',mailbox:chat,dispatchId:null,
      messages:[{messageId:'worker-result',runId:'run',from:'dispatch:worker-dispatch'}]}}
    promptText = 'RAW WORKER REPORT: must never become speech'
    return {ok:true}
  }
  if (message.op === 'legacy-final') {
    const opener = prompt()
    const turn = {...opener,itemId:'turn',body:{kind:'turn',turnId:'provider-turn',userItemId:opener.itemId,state:'completed',outcome:'success'}}
    const response = {...opener,itemId:'reply',turnScope:{kind:'turn',turnItemId:'turn'},
      body:{kind:'message',role:'assistant',blocks:[{type:'text',text:'My analyzed final answer.'}]}}
    await deliverDesktopVoiceContinuation(runtime, chat, 'provider-turn', {items:[opener,turn,response],openerIds:new Map()})
    return {ok:true}
  }
  if (message.op === 'task-mode') return {active: desktopVoiceTasks.taskMode({chat, request})}
  throw new Error(`Unknown operation ${message.op}`)
}

const server = createServer((socket) => {
  const lines = createInterface({input: socket})
  lines.on('line', (line) => {
    void handle(JSON.parse(line)).then(
      (result) => socket.end(JSON.stringify(result) + '\n'),
      (error) => socket.end(JSON.stringify({ok:false, error: String(error)}) + '\n'))
  })
})
server.listen(process.env.VOICE_TEST_SOCKET, () => { console.log('SERVER_READY') })
process.on('SIGTERM', () => { server.close(); process.exit(0) })
