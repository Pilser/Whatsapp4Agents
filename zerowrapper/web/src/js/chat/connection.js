import { els } from './dom.js'
import { state as authState, saveToStorage } from './auth.js'
import { fetchHistory, addMessage, abortHistoryFetch } from './messages.js'
import * as streaming from './streaming.js'
import * as toolCalls from './tool-calls.js'

export const state = {
  ws: null,
  connected: false,
}

function attachHandlers(ws) {
  ws.onopen = () => {
    state.connected = true
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
          break
        case 'error':
          addMessage('error', data.message || data.error || 'Unknown error')
          break
      }
    } catch (e2) {
      addMessage('assistant', e.data)
    }
  }

  ws.onclose = () => {
    state.connected = false
    streaming.reset()
    toolCalls.clearPending()
    if (els.hint) els.hint.style.display = 'block'
  }

  ws.onerror = () => {
    state.connected = false
    streaming.reset()
    toolCalls.clearPending()
    if (els.hint) els.hint.style.display = 'block'
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

export function send(text) {
  if (!state.connected || !state.ws) {
    addMessage('error', 'Not connected to backend.')
    return
  }
  state.ws.send(JSON.stringify({
    session_id: authState.sessionId || '',
    user_id: authState.phone,
    content: text,
  }))
}
