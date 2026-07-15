import { els } from './dom.js'
import { state as authState, saveToStorage } from './auth.js'
import { fetchHistory, addMessage, abortHistoryFetch } from './messages.js'
import * as streaming from './streaming.js'
import * as toolCalls from './tool-calls.js'

export const state = {
  ws: null,
  connected: false,
}

export let onStateChange = null
export function setOnStateChange(cb) { onStateChange = cb }

function attachHandlers(ws) {
  ws.onopen = () => {
    state.connected = true
    console.log('[ws] connected to', ws.url)
    if (els.hint) els.hint.style.display = 'none'
    fetchHistory()
  }

  ws.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data)
      switch (data.type) {
        case 'session_created':
          if (data.session_id && data.session_id !== authState.sessionId) {
            authState.sessionId = data.session_id
            saveToStorage()
          }
          break
        case 'thinking':
          streaming.handleThinking(data.content)
          break
        case 'chunk':
          streaming.handleChunk(data.content)
          break
        case 'chunk_reset':
          break
        case 'tool_call':
          streaming.finalizeIfActive()
          toolCalls.handleToolCall(data.tool_call_id, data.name, data.args)
          break
        case 'tool_result':
          toolCalls.handleToolResult(data.tool_call_id, data.name, data.output)
          break
        case 'done':
          streaming.handleDone(data.content)
          if (onStateChange) onStateChange('done')
          break
        case 'error':
          addMessage('error', data.message || data.error || 'Unknown error')
          if (onStateChange) onStateChange('error')
          break
      }
    } catch (e2) {
      addMessage('assistant', e.data)
    }
  }

  ws.onclose = (e) => {
    state.connected = false
    streaming.reset()
    toolCalls.clearPending()
    console.warn('[ws] closed: code=%s reason="%s" wasClean=%s', e.code, e.reason, e.wasClean)
    if (els.hint) els.hint.style.display = 'block'
    if (onStateChange) onStateChange('disconnect')
  }

  ws.onerror = (e) => {
    state.connected = false
    streaming.reset()
    toolCalls.clearPending()
    console.error('[ws] error event:', e.type, 'readyState:', ws.readyState)
    if (els.hint) els.hint.style.display = 'block'
    if (onStateChange) onStateChange('disconnect')
  }
}

export function connect() {
  if (state.ws) {
    state.ws.close()
    streaming.reset()
    toolCalls.clearPending()
  }
  abortHistoryFetch()

  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(protocol + '//' + location.host + '/ws')
  state.ws = ws
  attachHandlers(ws)
}

export function disconnect() {
  if (state.ws) {
    state.ws.onclose = null
    state.ws.onerror = null
    state.ws.close()
    state.ws = null
  }
  state.connected = false
  streaming.reset()
  toolCalls.clearPending()
  abortHistoryFetch()
}

export function send(text, role, attachments) {
  if (!state.connected || !state.ws) {
    addMessage('error', 'Not connected to backend.')
    return
  }
  const payload = {
    session_id: authState.sessionId || '',
    user_id: authState.phone,
    content: text,
  }
  if (role) payload.role = role
  if (attachments && attachments.length > 0) payload.attachments = attachments
  state.ws.send(JSON.stringify(payload))
}

export function sendCancel() {
  if (!state.connected || !state.ws) return
  state.ws.send(JSON.stringify({ type: 'cancel' }))
}
