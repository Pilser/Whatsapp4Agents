import { createConnectionManager } from './network.js'

const network = createConnectionManager()

const els = {
  input: document.getElementById('chat-input'),
  sendBtn: document.getElementById('send-btn'),
  messages: document.getElementById('messages'),
  userInfo: document.getElementById('user-info'),
  authBtn: document.getElementById('auth-btn'),
}

let phone = ''
let sessionId = ''
let typing = false

function addMessage(role, content) {
  const div = document.createElement('div')
  div.className = `flex ${role === 'user' ? 'justify-end' : 'justify-start'}`
  const bubble = document.createElement('div')
  bubble.className = `max-w-[75%] rounded-2xl px-4 py-2 text-sm ${
    role === 'user'
      ? 'bg-primary text-primary-foreground'
      : 'bg-foreground/5 text-foreground'
  }`
  bubble.textContent = content
  div.appendChild(bubble)
  els.messages.appendChild(div)
  els.messages.scrollTop = els.messages.scrollHeight
}

function showTyping() {
  typing = true
  const el = document.createElement('div')
  el.id = 'typing-indicator'
  el.className = 'flex items-center gap-1 px-4 py-2'
  el.innerHTML = '<span class="size-1.5 rounded-full bg-muted-foreground"></span>'.repeat(3) + '<span class="text-xs text-muted-foreground">typing...</span>'
  els.messages.appendChild(el)
  els.messages.scrollTop = els.messages.scrollHeight
}

function hideTyping() {
  typing = false
  document.getElementById('typing-indicator')?.remove()
}

function connect() {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  network.connect(protocol + '//' + location.host + '/ws', {
    onOpen: () => { fetchHistory() },
    onMessage: (e) => {
      try {
        const data = JSON.parse(e.data)
        switch (data.type) {
          case 'session_created':
            if (data.session_id) sessionId = data.session_id
            break
          case 'thinking':
            hideTyping(); showTyping()
            break
          case 'chunk':
            hideTyping()
            break
          case 'done':
            hideTyping(); addMessage('assistant', data.content || '')
            break
          case 'error':
            hideTyping(); addMessage('error', data.message || data.error || 'Unknown error')
            break
        }
      } catch (e2) { addMessage('assistant', e2.data) }
    },
    onClose: () => { hideTyping() },
  })
}

async function fetchHistory() {
  if (!phone) return
  try {
    const res = await fetch(`/api/history?user_id=${encodeURIComponent(phone)}`)
    const data = await res.json()
    sessionId = data.session_id || ''
    els.messages.innerHTML = ''
    for (const msg of data.messages || []) {
      if (msg.role === 'user' || msg.role === 'assistant') addMessage(msg.role, msg.content || '')
    }
  } catch (e) { console.error('fetchHistory error:', e) }
}

function sendMessage() {
  const text = els.input.value.trim()
  if (!text) return
  addMessage('user', text)
  network.send(JSON.stringify({
    session_id: sessionId,
    user_id: phone,
    content: text,
    role: 'public',
  }))
  els.input.value = ''
  showTyping()
}

els.sendBtn.addEventListener('click', sendMessage)
els.input.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendMessage() })

els.authBtn.addEventListener('click', async () => {
  const p = prompt('Enter your phone number:')
  if (!p) return
  phone = p
  els.userInfo.textContent = phone
  els.authBtn.textContent = 'Signed in'
  els.authBtn.disabled = true
  connect()
})
