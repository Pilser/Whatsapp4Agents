import * as network from './network.js'
import { marked } from 'marked'
import { render as renderMarkdown } from './markdown.js'
import { USER_AVATAR, BOT_AVATAR, escapeHtml } from './constants.js'

export const state = {
  sessionId: '',
  phone: '',
  role: '',
}

let streamingContent = ''
let streamingEl = null
let onFinalize = null
let typingTimer = null
let pendingToolCalls = new Map()
let thinkingContent = ''

function getEl(id) { return document.getElementById(id) }
function getContainer() { return getEl('owner-messages') }
function getTyping() { return getEl('owner-typing') }
function getHint() { return getEl('owner-hint') }

function showOwnerDisconnected() {
  const hint = getHint()
  if (!hint) return
  const p = hint.querySelector('p')
  if (p) p.textContent = 'Backend not connected. Check your connection and refresh.'
  const btn = getEl('owner-refresh-btn')
  if (btn) btn.classList.remove('hidden')
  hint.style.display = 'flex'
}

function extractAttachments(raw) {
  if (!raw || !raw.startsWith('{')) return null
  try {
    const p = JSON.parse(raw)
    if (p.attachments && Array.isArray(p.attachments) && p.attachments.length > 0) {
      return p.attachments
    }
  } catch (e) {}
  return null
}

function extractMessageText(raw) {
  if (!raw) return ''
  if (raw.startsWith('{')) {
    try {
      const p = JSON.parse(raw)
      if (p.message) return p.message
    } catch (e) {}
  }
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
      chip.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/><polyline points="14 2 14 8 20 8"/></svg> ' + escapeHtml(a.filename || a.name || 'file')
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
    content.className = 'msg-content chat-markdown'
    renderMarkdown(content, marked.parse(text || ''))
    row.appendChild(content)
  }
  return row
}

export function addMessage(role, text) {
  const container = getContainer()
  if (!container) return
  const hint = getHint()
  if (hint) hint.style.display = 'none'
  const row = buildMessageRow(role, text)
  container.appendChild(row)
  row.scrollIntoView({ block: 'end' })
}

export function showTyping() {
  const el = getTyping()
  if (el) el.classList.remove('hidden')
}

function hideTyping() {
  const el = getTyping()
  if (el) el.classList.add('hidden')
}

function resetStreaming() {
  streamingContent = ''
  streamingEl = null
  thinkingContent = ''
  if (typingTimer) { clearTimeout(typingTimer); typingTimer = null }
  hideTyping()
}

export function finalizeStreaming() {
  if (streamingEl) {
    const text = streamingContent
    const thinkText = thinkingContent
    streamingContent = ''
    streamingEl = null
    thinkingContent = ''
    const container = getContainer()
    const existing = container ? container.querySelector('.message-row.streaming') : null
    if (existing) existing.remove()

    if (thinkText || text.trim()) {
      const row = document.createElement('div')
      row.className = 'message-row assistant'
      const avatar = document.createElement('div')
      avatar.className = 'msg-avatar'
      avatar.innerHTML = BOT_AVATAR
      row.appendChild(avatar)
      const content = document.createElement('div')
      content.className = 'msg-content chat-markdown'

      let html = ''
      if (thinkText) {
        html += '<div class="thinking-block"><details><summary>+ thoughts</summary><pre>' + escapeHtml(thinkText) + '</pre></details></div>'
      }
      if (text.trim()) {
        html += marked.parse(text.trim())
      }
      renderMarkdown(content, html)
      row.appendChild(content)
      if (container) {
        container.appendChild(row)
        row.scrollIntoView({ block: 'end' })
      }
    }
  }
}

function handleThinking(content) {
  thinkingContent += (content || '')
  showTyping()
  if (typingTimer) clearTimeout(typingTimer)
  typingTimer = setTimeout(() => { hideTyping() }, 30000)
}

function handleChunk(content) {
  hideTyping()
  if (typingTimer) { clearTimeout(typingTimer); typingTimer = null }
  if (!streamingEl) {
    streamingContent = ''
    thinkingContent = ''
    streamingEl = document.createElement('div')
    streamingEl.className = 'message-row assistant streaming'
    const avatar = document.createElement('div')
    avatar.className = 'msg-avatar'
    avatar.innerHTML = BOT_AVATAR
    streamingEl.appendChild(avatar)
    const contentDiv = document.createElement('div')
    contentDiv.className = 'msg-content chat-markdown'
    streamingEl.appendChild(contentDiv)
    const container = getContainer()
    if (container) {
      container.appendChild(streamingEl)
      streamingEl.scrollIntoView({ block: 'end' })
    }
  }
  streamingContent += content
  const contentDiv = streamingEl.querySelector('.msg-content')
  if (contentDiv) {
    let html = ''
    if (thinkingContent) {
      html += '<div class="thinking-block"><details open><summary>+ thoughts</summary><pre>' + escapeHtml(thinkingContent) + '</pre></details></div>'
    }
    if (streamingContent) {
      html += marked.parse(streamingContent)
    }
    renderMarkdown(contentDiv, html)
  }
}

function handleDone(content) {
  if (content && content.trim()) {
    streamingContent = content
  }
  finalizeStreaming()
  if (onFinalize) { onFinalize(); onFinalize = null }
  document.dispatchEvent(new CustomEvent('owner-stream-done'))
}

function handleToolCall(id, name, args) {
  finalizeStreaming()
  const container = getContainer()
  if (!container) return
  const hint = getHint()
  if (hint) hint.style.display = 'none'
  const row = document.createElement('div')
  row.className = 'message-row tool-call'
  row.dataset.toolCallId = id
  const avatar = document.createElement('div')
  avatar.className = 'msg-avatar'
  avatar.innerHTML = BOT_AVATAR
  row.appendChild(avatar)
  const content = document.createElement('div')
  content.className = 'msg-content'
  const argsStr = typeof args === 'string' ? args : JSON.stringify(args, null, 2)
  content.innerHTML = '<details class="tool-call-details">' +
    '<summary><span class="tc-pill-status running">\u25CC</span> + acting</summary>' +
    '<p style="margin:4px 0 2px;font-weight:600;font-size:0.8rem;">' + escapeHtml(name) + '</p>' +
    '<pre>' + escapeHtml(argsStr) + '</pre>' +
  '</details>'
  row.appendChild(content)
  container.appendChild(row)
  row.scrollIntoView({ block: 'end' })
  pendingToolCalls.set(id, row)
}

function handleToolResult(id, name, output) {
  const row = pendingToolCalls.get(id)
  if (!row) return
  const details = row.querySelector('.tool-call-details')
  if (details) {
    const summary = details.querySelector('summary')
    if (summary) {
      summary.innerHTML = '<span class="tc-pill-status done">\u2713</span> + acting'
    }
    if (output) {
      const existingPre = details.querySelector('pre')
      if (existingPre) existingPre.remove()
      const pre = document.createElement('pre')
      if (output.length > 80) {
        pre.innerHTML = '<details><summary>' + escapeHtml(output.slice(0, 80)) + '...</summary><pre>' + escapeHtml(output) + '</pre></details>'
      } else {
        pre.textContent = output
      }
      details.appendChild(pre)
    }
  }
  pendingToolCalls.delete(id)
}

function ownerFetchHistory() {
  if (!state.phone) return
  const url = '/api/history?user_id=' + encodeURIComponent(state.phone) + '&role=' + encodeURIComponent(state.role)
  fetch(url)
    .then(r => r.json())
    .then(data => {
      if (data.session_id) {
        state.sessionId = data.session_id
      }
      if (!data.messages || data.messages.length === 0) return
      const container = getContainer()
      if (!container) return
      const hint = getHint()
      if (hint) hint.style.display = 'none'
      const frag = document.createDocumentFragment()
      data.messages.forEach(msg => {
        if (msg.role === 'user' || msg.role === 'assistant') {
          const text = msg.role === 'assistant' ? msg.content : msg.content
          frag.appendChild(buildMessageRow(msg.role, text))
        }
      })
      container.appendChild(frag)
      const last = container.lastElementChild
      if (last) last.scrollIntoView({ block: 'end' })
    })
    .catch(() => {})
}

export function connectOwnerWs() {
  if (network.getState() !== network.State.DISCONNECTED) network.disconnect()
  resetStreaming()
  pendingToolCalls.clear()

  const refreshBtn = getEl('owner-refresh-btn')
  if (refreshBtn) {
    refreshBtn.addEventListener('click', () => location.reload())
  }

  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const url = protocol + '//' + location.host + '/ws'

  network.connect(url, {
    onOpen: () => {
      const hint = getHint()
      if (hint) hint.style.display = 'none'
      const btn = getEl('owner-refresh-btn')
      if (btn) btn.classList.add('hidden')
      ownerFetchHistory()
    },
    onMessage: (e) => {
      try {
        const data = JSON.parse(e.data)
        switch (data.type) {
          case 'session_created':
            if (data.session_id && data.session_id !== state.sessionId) state.sessionId = data.session_id
            break
          case 'thinking':
            handleThinking(data.content)
            break
          case 'chunk':
            handleChunk(data.content)
            break
          case 'tool_call':
            handleToolCall(data.tool_call_id, data.name, data.args)
            break
          case 'tool_result':
            handleToolResult(data.tool_call_id, data.name, data.output)
            break
          case 'done':
            handleDone(data.content)
            break
          case 'error':
            addMessage('error', data.message || data.error || 'Unknown error')
            break
        }
      } catch (e2) {
        addMessage('assistant', e2.data)
      }
    },
    onClose: () => {
      resetStreaming()
      pendingToolCalls.clear()
      showOwnerDisconnected()
    },
  })
}

export function disconnectOwnerWs() {
  network.disconnect()
  resetStreaming()
  pendingToolCalls.clear()
}

export function sendOwnerMessage(text, attachments) {
  if (network.getState() !== network.State.CONNECTED) { addMessage('error', 'Not connected to backend.'); return }
  const payload = {
    session_id: state.sessionId || '',
    user_id: state.phone,
    content: text,
    role: state.role,
  }
  if (attachments && attachments.length > 0) {
    payload.attachments = attachments.map(a => ({ filename: a.filename, mime_type: a.mime_type, path: a.path }))
  }
  network.send(JSON.stringify(payload))
}

export function sendOwnerCancel() {
  network.sendCancel(state.sessionId)
}

export function setOnFinalize(fn) { onFinalize = fn }
