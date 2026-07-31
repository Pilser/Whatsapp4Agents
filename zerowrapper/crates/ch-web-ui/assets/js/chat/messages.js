import { marked } from 'marked'
import { els } from './dom.js'
import { USER_AVATAR, BOT_AVATAR } from './constants.js'
import { render as renderMarkdown } from './markdown.js'
import { finalize } from './streaming.js'

let abortController = null

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
    content.textContent = text
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

export function addMessage(role, text, scroll = true) {
  finalize()

  const row = buildMessageRow(role, text)
  els.messages.appendChild(row)
  if (els.hint && els.hint.style.display !== 'none') els.hint.style.display = 'none'
  if (scroll) {
    row.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }
}

export function scrollToBottom() {
  const lastMsg = els.messages.lastElementChild
  if (lastMsg) {
    lastMsg.scrollIntoView({ block: 'end' })
  }
}

export function fetchHistory() {
  if (abortController) abortController.abort()
  abortController = new AbortController()

  const phone = localStorage.getItem('zerowrapper_phone')
  if (!phone) return

  const url = '/api/history?user_id=' + encodeURIComponent(phone)
  fetch(url, { signal: abortController.signal })
    .then(r => r.json())
    .then(data => {
      if (data.session_id) {
        localStorage.setItem('zerowrapper_session', data.session_id)
      }
      if (!data.messages || data.messages.length === 0) return
      const frag = document.createDocumentFragment()
      data.messages.forEach(msg => {
        if (msg.role === 'user' || msg.role === 'assistant') {
          const row = buildMessageRow(msg.role, msg.role === 'assistant' ? msg.content : extractMessageText(msg.content))
          if (row) frag.appendChild(row)
        }
      })
      els.messages.appendChild(frag)
      scrollToBottom()
    })
    .catch(() => {})
}

export function abortHistoryFetch() {
  if (abortController) {
    abortController.abort()
    abortController = null
  }
}
