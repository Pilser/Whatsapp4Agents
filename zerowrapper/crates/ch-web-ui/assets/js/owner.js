const State = { DISCONNECTED: 0, CONNECTING: 1, CONNECTED: 2, RECONNECTING: 3 }

let ws = null
let wsUrl = ''
let wsHandlers = {}
let reconnectTimer = null
let reconnectAttempts = 0
const MAX_RECONNECT_DELAY = 30000

function getState() {
  if (!ws) return State.DISCONNECTED
  switch (ws.readyState) {
    case WebSocket.CONNECTING: return State.CONNECTING
    case WebSocket.OPEN: return State.CONNECTED
    case WebSocket.CLOSING: return State.DISCONNECTED
    case WebSocket.CLOSED: return State.DISCONNECTED
    default: return State.DISCONNECTED
  }
}

const network = {
  State,
  getState,
  connect(url, handlers) {
    if (ws) network.disconnect()
    wsUrl = url
    wsHandlers = handlers || {}
    reconnectAttempts = 0
    doConnect()
  },
  disconnect() {
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null }
    if (ws) { ws.onclose = null; ws.onmessage = null; ws.onerror = null; ws.close(); ws = null }
    if (wsHandlers.onClose) wsHandlers.onClose()
  },
  send(data) { if (ws && ws.readyState === WebSocket.OPEN) ws.send(data) },
  sendCancel(sessionId) {
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: 'cancel', session_id: sessionId || '' }))
  },
}

function doConnect() {
  ws = new WebSocket(wsUrl)
  ws.onopen = () => { reconnectAttempts = 0; if (wsHandlers.onOpen) wsHandlers.onOpen() }
  ws.onmessage = (e) => { if (wsHandlers.onMessage) wsHandlers.onMessage(e) }
  ws.onclose = (e) => {
    if (e.code !== 1000) scheduleReconnect()
    if (wsHandlers.onClose) wsHandlers.onClose()
  }
  ws.onerror = () => {}
}

function scheduleReconnect() {
  if (reconnectTimer) return
  reconnectAttempts++
  const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), MAX_RECONNECT_DELAY)
  reconnectTimer = setTimeout(() => { reconnectTimer = null; doConnect() }, delay)
}

const uploadState = { pending: [] }
async function uploadFile(file) {
  const formData = new FormData()
  formData.append('file', file)
  const res = await fetch('/api/upload', { method: 'POST', headers: { 'X-Filename': file.name }, body: formData })
  if (!res.ok) throw new Error('Upload failed: ' + res.statusText)
  const info = await res.json()
  return { filename: file.name, mime_type: file.type || 'application/octet-stream', path: info.path || info.filename }
}
function createAttachmentChip(info) {
  const chip = document.createElement('span')
  chip.className = 'attachment-chip'
  chip.textContent = info.filename
  return chip
}

/* --- Owner State & Storage --- */
const STORAGE_PHONE = 'zerowrapper_owner_phone'
const STORAGE_NAME = 'zerowrapper_owner_name'
const STORAGE_ROLE = 'zerowrapper_owner_role'
const STORAGE_SESSION = 'zerowrapper_owner_session'
const state = { phone: '', displayName: '', role: '' }
let sessionId = ''
let streamingContent = ''
let streamingEl = null
let onFinalize = null
let typingTimer = null
let pendingToolCalls = new Map()
let thinkingContent = ''
const USER_AVATAR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>'
const BOT_AVATAR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>'
const THINK_ICON = '<svg class="think-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>'
const TOOL_ICON = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>'

function escapeHtml(str) { const d = document.createElement('div'); d.appendChild(document.createTextNode(str)); return d.innerHTML }
function getEl(id) { return document.getElementById(id) }

function loadFromStorage() {
  state.phone = localStorage.getItem(STORAGE_PHONE) || ''
  state.displayName = localStorage.getItem(STORAGE_NAME) || ''
  state.role = localStorage.getItem(STORAGE_ROLE) || ''
  sessionId = localStorage.getItem(STORAGE_SESSION) || ''
}
function saveToStorage() {
  if (state.phone) localStorage.setItem(STORAGE_PHONE, state.phone)
  if (state.displayName) localStorage.setItem(STORAGE_NAME, state.displayName)
  if (state.role) localStorage.setItem(STORAGE_ROLE, state.role)
  if (sessionId) localStorage.setItem(STORAGE_SESSION, sessionId)
}
function clearFromStorage() { ;[STORAGE_PHONE, STORAGE_NAME, STORAGE_ROLE, STORAGE_SESSION].forEach(k => localStorage.removeItem(k)) }

function extractAttachments(raw) {
  if (!raw || !raw.startsWith('{')) return null
  try { const p = JSON.parse(raw); if (p.attachments && Array.isArray(p.attachments) && p.attachments.length > 0) return p.attachments } catch (e) {}
  return null
}
function extractMessageText(raw) {
  if (!raw || !raw.startsWith('{')) return raw
  try { const p = JSON.parse(raw); if (p.message) return p.message } catch (e) {}
  return raw
}

function renderTextWithAttachments(text) {
  const div = document.createElement('div')
  const attachments = extractAttachments(text)
  const msgText = extractMessageText(text)
  if (attachments) {
    const fileList = document.createElement('div')
    fileList.className = 'msg-attachments'
    attachments.forEach(a => {
      const chip = document.createElement('span')
      chip.className = 'attachment-chip'
      chip.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/><polyline points="14 2 14 8 20 8"/></svg> ' + escapeHtml(a.filename || a.name || 'file')
      fileList.appendChild(chip)
    })
    div.appendChild(fileList)
  }
  if (msgText) {
    const p = document.createElement('p')
    p.style.margin = attachments ? '6px 0 0' : '0'
    p.textContent = msgText
    div.appendChild(p)
  }
  return div
}

function buildMessageRow(role, text) {
  const row = document.createElement('div')
  row.className = 'message-row ' + role
  if (role === 'error') {
    const content = document.createElement('div')
    content.className = 'msg-content'
    content.textContent = text
    row.appendChild(content)
  } else if (role === 'user') {
    const avatar = document.createElement('div')
    avatar.className = 'msg-avatar'
    avatar.innerHTML = USER_AVATAR
    row.appendChild(avatar)
    const content = document.createElement('div')
    content.className = 'msg-content'
    content.appendChild(renderTextWithAttachments(text))
    row.appendChild(content)
  } else {
    const avatar = document.createElement('div')
    avatar.className = 'msg-avatar'
    avatar.innerHTML = BOT_AVATAR
    row.appendChild(avatar)
    const content = document.createElement('div')
    content.className = 'msg-content'
    content.textContent = text || ''
    row.appendChild(content)
  }
  return row
}

function addMessage(role, text) {
  const container = getEl('owner-messages')
  if (!container) return
  const hint = getEl('owner-hint')
  if (hint) hint.style.display = 'none'
  container.appendChild(buildMessageRow(role, text))
  container.lastElementChild.scrollIntoView({ block: 'end' })
}
function showTyping() { const el = getEl('owner-typing'); if (el) el.classList.remove('hidden') }
function hideTyping() { const el = getEl('owner-typing'); if (el) el.classList.add('hidden') }

function resetStreaming() {
  streamingContent = ''; streamingEl = null; thinkingContent = ''
  if (typingTimer) { clearTimeout(typingTimer); typingTimer = null }
  hideTyping()
}

function finalizeStreaming() {
  if (!streamingEl) return
  const text = streamingContent; const thinkText = thinkingContent
  streamingContent = ''; streamingEl = null; thinkingContent = ''
  const container = getEl('owner-messages')
  const existing = container ? container.querySelector('.message-row.streaming') : null
  if (existing) existing.remove()
  if (thinkText || (text && text.trim())) {
    const row = document.createElement('div')
    row.className = 'message-row assistant'
    const avatar = document.createElement('div')
    avatar.className = 'msg-avatar'
    avatar.innerHTML = BOT_AVATAR
    row.appendChild(avatar)
    const content = document.createElement('div')
    content.className = 'msg-content'
    let html = ''
    if (thinkText) html += '<div class="thinking-block"><details><summary>' + THINK_ICON + ' Thinking</summary><pre>' + escapeHtml(thinkText) + '</pre></details></div>'
    if (text && text.trim()) html += '<p>' + escapeHtml(text.trim()) + '</p>'
    content.innerHTML = html
    row.appendChild(content)
    if (container) { container.appendChild(row); row.scrollIntoView({ block: 'end' }) }
  }
}

function handleThinking(content) { thinkingContent += (content || ''); showTyping(); if (typingTimer) clearTimeout(typingTimer); typingTimer = setTimeout(() => hideTyping(), 30000) }

function handleChunk(content) {
  hideTyping(); if (typingTimer) { clearTimeout(typingTimer); typingTimer = null }
  if (!streamingEl) {
    streamingContent = ''; thinkingContent = ''
    streamingEl = document.createElement('div')
    streamingEl.className = 'message-row assistant streaming'
    const avatar = document.createElement('div'); avatar.className = 'msg-avatar'; avatar.innerHTML = BOT_AVATAR
    streamingEl.appendChild(avatar)
    const contentDiv = document.createElement('div'); contentDiv.className = 'msg-content'
    streamingEl.appendChild(contentDiv)
    const container = getEl('owner-messages')
    if (container) { container.appendChild(streamingEl); streamingEl.scrollIntoView({ block: 'end' }) }
  }
  streamingContent += content
  const contentDiv = streamingEl.querySelector('.msg-content')
  if (contentDiv) {
    let html = ''
    if (thinkingContent) html += '<div class="thinking-block"><details open><summary>' + THINK_ICON + ' Thinking</summary><pre>' + escapeHtml(thinkingContent) + '</pre></details></div>'
    if (streamingContent) html += '<p>' + escapeHtml(streamingContent) + '</p>'
    contentDiv.innerHTML = html
  }
}

function handleDone(content) { if (content && content.trim()) streamingContent = content; finalizeStreaming(); if (onFinalize) { onFinalize(); onFinalize = null } }

function handleToolCall(id, name, args) {
  finalizeStreaming()
  const container = getEl('owner-messages')
  if (!container) return
  const hint = getEl('owner-hint')
  if (hint) hint.style.display = 'none'
  const row = document.createElement('div')
  row.className = 'message-row tool-call'; row.dataset.toolCallId = id
  const avatar = document.createElement('div'); avatar.className = 'msg-avatar'; avatar.innerHTML = TOOL_ICON
  row.appendChild(avatar)
  const content = document.createElement('div'); content.className = 'msg-content'
  const argsStr = typeof args === 'string' ? args : JSON.stringify(args, null, 2)
  content.innerHTML = '<details class="tool-call-details"><summary><span class="tc-pill-status running">\u25CC</span> <span style="opacity:0.6">\uD83D\uDD27</span> Tool Call</summary><p style="margin:4px 0 2px;font-weight:600;font-size:0.8rem;">' + escapeHtml(name) + '</p><pre>' + escapeHtml(argsStr) + '</pre></details>'
  row.appendChild(content)
  container.appendChild(row); row.scrollIntoView({ block: 'end' })
  pendingToolCalls.set(id, row)
}

function handleToolResult(id, name, output) {
  const row = pendingToolCalls.get(id)
  if (!row) return
  const details = row.querySelector('.tool-call-details')
  if (details) {
    const summary = details.querySelector('summary')
    if (summary) summary.innerHTML = '<span class="tc-pill-status done">\u2713</span> <span style="opacity:0.6">\uD83D\uDD27</span> Tool Call'
    if (output) {
      const existingPre = details.querySelector('pre')
      if (existingPre) existingPre.remove()
      const pre = document.createElement('pre')
      if (output.length > 80) pre.innerHTML = '<details><summary>' + escapeHtml(output.slice(0, 80)) + '...</summary><span>' + escapeHtml(output) + '</span></details>'
      else pre.textContent = output
      details.appendChild(pre)
    }
  }
  pendingToolCalls.delete(id)
}

function fetchHistory() {
  if (!state.phone) return
  fetch('/api/history?user_id=' + encodeURIComponent(state.phone) + '&role=' + encodeURIComponent(state.role))
    .then(r => r.json()).then(data => {
      if (data.session_id) sessionId = data.session_id
      if (!data.messages || data.messages.length === 0) return
      const container = getEl('owner-messages')
      if (!container) return
      const hint = getEl('owner-hint'); if (hint) hint.style.display = 'none'
      const frag = document.createDocumentFragment()
      data.messages.forEach(msg => { if (msg.role === 'user' || msg.role === 'assistant') frag.appendChild(buildMessageRow(msg.role, msg.content)) })
      container.appendChild(frag)
      const last = container.lastElementChild; if (last) last.scrollIntoView({ block: 'end' })
    }).catch(() => {})
}

function connectWs() {
  if (network.getState() !== State.DISCONNECTED) network.disconnect()
  resetStreaming(); pendingToolCalls.clear()
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  network.connect(protocol + '//' + location.host + '/ws', {
    onOpen: () => { const hint = getEl('owner-hint'); if (hint) hint.style.display = 'none'; fetchHistory() },
    onMessage: (e) => {
      try {
        const data = JSON.parse(e.data)
        switch (data.type) {
          case 'session_created': if (data.session_id && data.session_id !== sessionId) sessionId = data.session_id; break
          case 'thinking': handleThinking(data.content); break
          case 'chunk': handleChunk(data.content); break
          case 'tool_call': handleToolCall(data.tool_call_id, data.name, data.args); break
          case 'tool_result': handleToolResult(data.tool_call_id, data.name, data.output); break
          case 'done': handleDone(data.content); break
          case 'error': addMessage('error', data.message || data.error || 'Unknown error'); break
        }
      } catch (e2) { addMessage('assistant', e2.data) }
    },
    onClose: () => { resetStreaming(); pendingToolCalls.clear() },
  })
}
function disconnectWs() { network.disconnect(); resetStreaming(); pendingToolCalls.clear() }

export function init() {
  getEl('owner-btn')?.addEventListener('click', showLogin)
  getEl('owner-btn-mobile')?.addEventListener('click', showLogin)
  getEl('owner-login-close')?.addEventListener('click', hideLogin)
  getEl('owner-login-btn')?.addEventListener('click', handleLogin)
  getEl('owner-back-btn')?.addEventListener('click', backToPublic)
  getEl('owner-password-input')?.addEventListener('keydown', (e) => { if (e.key === 'Enter') getEl('owner-login-btn')?.click() })
  getEl('owner-contact-link')?.addEventListener('click', () => { getEl('owner-login-form')?.classList.add('hidden'); getEl('owner-contact-info')?.classList.remove('hidden'); const t = getEl('owner-login-title'); if (t) t.textContent = 'Contact' })
  getEl('owner-contact-back')?.addEventListener('click', () => { getEl('owner-contact-info')?.classList.add('hidden'); getEl('owner-login-form')?.classList.remove('hidden'); const t = getEl('owner-login-title'); if (t) t.textContent = 'Owner / Admin Login' })
  loadFromStorage()
  if (state.phone && state.role) showOwnerChat()
  document.addEventListener('owner-stream-done', () => showOwnerSendIcon())
}

function showLogin() { getEl('owner-login-overlay')?.classList.remove('hidden') }
function hideLogin() {
  getEl('owner-login-overlay')?.classList.add('hidden')
  const p = getEl('owner-phone-input'); if (p) p.value = ''
  const pw = getEl('owner-password-input'); if (pw) pw.value = ''
  const err = getEl('owner-login-error'); if (err) err.classList.add('hidden')
}

async function handleLogin() {
  const phone = getEl('owner-phone-input')?.value.trim() || ''
  const password = getEl('owner-password-input')?.value.trim() || ''
  const errEl = getEl('owner-login-error')
  if (!phone || !password) { if (errEl) { errEl.textContent = 'Please enter phone and password'; errEl.classList.remove('hidden') }; return }
  try {
    const res = await fetch('/api/admin/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ phone, password }) })
    const data = await res.json()
    if (!data.ok) { if (errEl) { errEl.textContent = data.error || 'Login failed'; errEl.classList.remove('hidden') }; return }
    state.phone = data.phone; state.displayName = data.name; state.role = data.role
    saveToStorage(); hideLogin(); showOwnerChat()
  } catch (err) { const errEl = getEl('owner-login-error'); if (errEl) { errEl.textContent = 'Network error'; errEl.classList.remove('hidden') } }
}

function showOwnerChat() {
  network.disconnect()
  const chatEl = getEl('owner-chat'); if (!chatEl) return
  getEl('page-root')?.classList.add('hidden')
  chatEl.classList.remove('hidden')
  const badge = getEl('owner-role-badge'); if (badge) badge.textContent = state.role
  setupOwnerForm(); connectWs()
}

function backToPublic() {
  disconnectWs(); teardownOwnerForm(); clearFromStorage()
  getEl('owner-chat')?.classList.add('hidden')
  getEl('page-root')?.classList.remove('hidden')
  const container = getEl('chat-messages')
  if (container) container.innerHTML = '<p id="fallback-hint" class="text-xs text-center" style="color: var(--color-text-weak);">Ask AI a question...</p>'
  const ac = getEl('owner-attachments'); if (ac) ac.innerHTML = ''
  uploadState.pending = []
}

function setupOwnerForm() {
  const form = getEl('owner-chat-form'); const input = getEl('owner-chat-input')
  const cancelBtn = getEl('owner-cancel-btn'); const uploadBtn = getEl('owner-upload-btn')
  const fileInput = getEl('owner-file-input')
  if (!form) return
  form._ownerHandler = (e) => {
    e.preventDefault()
    if (network.getState() !== State.CONNECTED) { addMessage('error', 'Not connected.'); return }
    finalizeStreaming()
    const text = input ? input.value.trim() : ''
    const attachments = [...uploadState.pending]
    if (!text && attachments.length === 0) { addMessage('error', 'Cannot send an empty message.'); return }
    showOwnerStopIcon()
    const combined = text && attachments.length > 0
      ? JSON.stringify({ message: text, attachments: attachments.map(a => ({ filename: a.filename, mime_type: a.mime_type, path: a.path })) })
      : (attachments.length > 0 ? JSON.stringify({ attachments: attachments.map(a => ({ filename: a.filename, mime_type: a.mime_type, path: a.path })) }) : text)
    addMessage('user', combined); sendMessage(text, attachments); showTyping()
    setOnFinalize(showOwnerSendIcon)
    if (input) { input.value = ''; input.style.height = 'auto' }
    uploadState.pending = []; const ac = getEl('owner-attachments'); if (ac) ac.innerHTML = ''
  }
  form.addEventListener('submit', form._ownerHandler)
  if (input) input.addEventListener('input', () => { input.style.height = 'auto'; input.style.height = Math.min(input.scrollHeight, 300) + 'px' })
  if (cancelBtn) cancelBtn.addEventListener('click', () => { sendCancel(); showOwnerSendIcon() })
  if (uploadBtn && fileInput) {
    uploadBtn.addEventListener('click', () => fileInput.click())
    fileInput.addEventListener('change', async () => {
      const files = Array.from(fileInput.files || [])
      for (const file of files) {
        try { const info = await uploadFile(file); uploadState.pending.push(info); getEl('owner-attachments')?.appendChild(createAttachmentChip(info)) }
        catch (err) { addMessage('error', 'Upload failed: ' + err.message) }
      }
      fileInput.value = ''
    })
  }
}
function teardownOwnerForm() { const form = getEl('owner-chat-form'); if (form && form._ownerHandler) { form.removeEventListener('submit', form._ownerHandler); delete form._ownerHandler } }
function showOwnerSendIcon() { const s = getEl('owner-send-btn'); const c = getEl('owner-cancel-btn'); if (s) s.style.display = 'flex'; if (c) c.style.display = 'none' }
function showOwnerStopIcon() { const s = getEl('owner-send-btn'); const c = getEl('owner-cancel-btn'); if (s) s.style.display = 'none'; if (c) c.style.display = 'flex' }
function sendMessage(text, attachments) {
  if (network.getState() !== State.CONNECTED) { addMessage('error', 'Not connected to backend.'); return }
  const payload = { session_id: sessionId || '', user_id: state.phone, content: text, role: state.role }
  if (attachments && attachments.length > 0) payload.attachments = attachments.map(a => ({ filename: a.filename, mime_type: a.mime_type, path: a.path }))
  network.send(JSON.stringify(payload))
}
function sendCancel() { network.sendCancel(sessionId) }
function setOnFinalize(fn) { onFinalize = fn }