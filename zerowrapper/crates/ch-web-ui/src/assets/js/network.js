export const State = {
  DISCONNECTED: 0,
  CONNECTING: 1,
  CONNECTED: 2,
  RECONNECTING: 3,
}

export function createConnectionManager() {
  let ws = null
  let state = State.DISCONNECTED
  let url = ''
  let handlers = { onOpen: null, onMessage: null, onClose: null }
  let reconnectTimer = null
  let reconnectDelay = 1000
  let manualDisconnect = false

  function setState(s) { state = s; dispatch('stateChange', s) }

  function connect(wsUrl, h) {
    url = wsUrl
    handlers = h
    manualDisconnect = false
    reconnectDelay = 1000
    doConnect()
  }

  function doConnect() {
    if (ws) { ws.onclose = null; ws.close() }
    setState(state === State.DISCONNECTED ? State.CONNECTING : State.RECONNECTING)
    try {
      ws = new WebSocket(url)
    } catch (e) {
      scheduleReconnect()
      return
    }
    ws.onopen = () => {
      setState(State.CONNECTED)
      reconnectDelay = 1000
      if (handlers.onOpen) handlers.onOpen()
    }
    ws.onmessage = (e) => { if (handlers.onMessage) handlers.onMessage(e) }
    ws.onclose = () => {
      if (!manualDisconnect) {
        setState(State.RECONNECTING)
        scheduleReconnect()
      } else {
        setState(State.DISCONNECTED)
      }
      if (handlers.onClose) handlers.onClose()
    }
    ws.onerror = () => {}
  }

  function scheduleReconnect() {
    clearTimeout(reconnectTimer)
    reconnectTimer = setTimeout(() => { doConnect() }, reconnectDelay)
    reconnectDelay = Math.min(reconnectDelay * 2, 30000)
  }

  function send(data) {
    if (ws && ws.readyState === WebSocket.OPEN) { ws.send(data) }
  }

  function sendCancel(sessionId) {
    send(JSON.stringify({ type: 'cancel', session_id: sessionId || '' }))
  }

  function disconnect() {
    manualDisconnect = true
    clearTimeout(reconnectTimer)
    if (ws) { ws.onclose = null; ws.close(); ws = null }
    setState(State.DISCONNECTED)
  }

  function getState() { return state }

  const listeners = {}
  function on(event, fn) { (listeners[event] ||= []).push(fn) }
  function dispatch(event, data) { (listeners[event] || []).forEach(fn => fn(data)) }

  window.addEventListener('online', () => {
    if (state === State.RECONNECTING || state === State.DISCONNECTED) {
      reconnectDelay = 1000; doConnect()
    }
  })

  return { connect, disconnect, send, sendCancel, getState, on, State, state: () => state }
}
