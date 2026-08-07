import * as network from './network.js'
import * as connection from './connection.js'
import { uploadFile, createAttachmentChip, uploadState } from './file-upload.js'
import * as ownerChat from './owner-chat.js'
import { fetchWithTimeout, setButtonLoading } from './fetch-util.js'

const STORAGE_PHONE = 'zerowrapper_owner_phone'
const STORAGE_NAME = 'zerowrapper_owner_name'
const STORAGE_ROLE = 'zerowrapper_owner_role'
const STORAGE_SESSION = 'zerowrapper_owner_session'

export const state = {
  phone: '',
  displayName: '',
  role: '',
}

function loadFromStorage() {
  state.phone = localStorage.getItem(STORAGE_PHONE) || ''
  state.displayName = localStorage.getItem(STORAGE_NAME) || ''
  state.role = localStorage.getItem(STORAGE_ROLE) || ''
  ownerChat.state.sessionId = localStorage.getItem(STORAGE_SESSION) || ''
}

function saveToStorage() {
  if (state.phone) localStorage.setItem(STORAGE_PHONE, state.phone)
  if (state.displayName) localStorage.setItem(STORAGE_NAME, state.displayName)
  if (state.role) localStorage.setItem(STORAGE_ROLE, state.role)
  if (ownerChat.state.sessionId) localStorage.setItem(STORAGE_SESSION, ownerChat.state.sessionId)
}

function clearFromStorage() {
  localStorage.removeItem(STORAGE_PHONE)
  localStorage.removeItem(STORAGE_NAME)
  localStorage.removeItem(STORAGE_ROLE)
  localStorage.removeItem(STORAGE_SESSION)
}

export function init() {
  document.getElementById('owner-btn')?.addEventListener('click', showLogin)
  document.getElementById('owner-btn-mobile')?.addEventListener('click', showLogin)
  document.getElementById('owner-login-close')?.addEventListener('click', hideLogin)
  document.getElementById('owner-login-btn')?.addEventListener('click', handleLogin)
  document.getElementById('owner-back-btn')?.addEventListener('click', backToPublic)
  document.getElementById('owner-password-input')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') document.getElementById('owner-login-btn')?.click()
  })

  loadFromStorage()
  if (state.phone && state.role) {
    showOwnerChat()
  }

  document.addEventListener('owner-stream-done', showOwnerSendIcon)
}

function setupOwnerForm() {
  const form = document.getElementById('owner-chat-form')
  const input = document.getElementById('owner-chat-input')
  const cancelBtn = document.getElementById('owner-cancel-btn')
  const uploadBtn = document.getElementById('owner-upload-btn')
  const fileInput = document.getElementById('owner-file-input')

  if (!form) return

  form._ownerHandler = (e) => {
    e.preventDefault()
    if (network.getState() !== network.State.CONNECTED) { ownerChat.addMessage('error', 'Not connected.'); return }
    ownerChat.finalizeStreaming()
    const text = input.value.trim()
    const attachments = [...uploadState.pending]
    if (!text && attachments.length === 0) { ownerChat.addMessage('error', 'Cannot send an empty message.'); return }
    showOwnerStopIcon()
    const combined = text && attachments.length > 0
      ? JSON.stringify({ message: text, attachments: attachments.map(a => ({ filename: a.filename, mime_type: a.mime_type, path: a.path })) })
      : (attachments.length > 0
        ? JSON.stringify({ attachments: attachments.map(a => ({ filename: a.filename, mime_type: a.mime_type, path: a.path })) })
        : text)
    ownerChat.addMessage('user', combined)
    ownerChat.sendOwnerMessage(text, attachments)
    ownerChat.showTyping()
    ownerChat.setOnFinalize(showOwnerSendIcon)
    input.value = ''
    input.style.height = 'auto'
    uploadState.pending = []
    const attachContainer = document.getElementById('owner-attachments')
    if (attachContainer) attachContainer.innerHTML = ''
  }

  form.addEventListener('submit', form._ownerHandler)

  input.addEventListener('input', autoGrow)

  if (cancelBtn) {
    cancelBtn.addEventListener('click', () => {
      ownerChat.sendOwnerCancel()
      showOwnerSendIcon()
    })
  }

  if (uploadBtn && fileInput) {
    uploadBtn.addEventListener('click', () => fileInput.click())
    fileInput.addEventListener('change', async () => {
      const files = Array.from(fileInput.files || [])
      for (const file of files) {
        try {
          const info = await uploadFile(file)
          uploadState.pending.push(info)
          const chip = createAttachmentChip(info)
          document.getElementById('owner-attachments')?.appendChild(chip)
        } catch (err) {
          ownerChat.addMessage('error', 'Upload failed: ' + err.message)
        }
      }
      fileInput.value = ''
    })
  }
}

function teardownOwnerForm() {
  const form = document.getElementById('owner-chat-form')
  if (form && form._ownerHandler) {
    form.removeEventListener('submit', form._ownerHandler)
    delete form._ownerHandler
  }
}

function autoGrow() {
  const input = document.getElementById('owner-chat-input')
  if (input) {
    input.style.height = 'auto'
    input.style.height = Math.min(input.scrollHeight, 300) + 'px'
  }
}

function showOwnerSendIcon() {
  const sendBtn = document.getElementById('owner-send-btn')
  const cancelBtn = document.getElementById('owner-cancel-btn')
  if (sendBtn) sendBtn.style.display = 'flex'
  if (cancelBtn) cancelBtn.style.display = 'none'
}

function showOwnerStopIcon() {
  const sendBtn = document.getElementById('owner-send-btn')
  const cancelBtn = document.getElementById('owner-cancel-btn')
  if (sendBtn) sendBtn.style.display = 'none'
  if (cancelBtn) cancelBtn.style.display = 'flex'
}

function showLogin() {
  document.getElementById('owner-login-overlay')?.classList.remove('hidden')
}

function hideLogin() {
  document.getElementById('owner-login-overlay')?.classList.add('hidden')
  document.getElementById('owner-phone-input').value = ''
  document.getElementById('owner-password-input').value = ''
  const err = document.getElementById('owner-login-error')
  if (err) err.classList.add('hidden')
}

async function handleLogin() {
  const phone = document.getElementById('owner-phone-input').value.trim()
  const password = document.getElementById('owner-password-input').value.trim()
  const errEl = document.getElementById('owner-login-error')

  if (!phone || !password) {
    if (errEl) { errEl.textContent = 'Please enter phone and password'; errEl.classList.remove('hidden') }
    return
  }

  const btn = document.getElementById('owner-login-btn')
  setButtonLoading(btn, true, 'Logging in…')
  try {
    const res = await fetchWithTimeout('/api/admin/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ phone, password }),
    })
    const data = await res.json()
    if (!data.ok) {
      if (errEl) { errEl.textContent = data.error || 'Login failed'; errEl.classList.remove('hidden') }
      return
    }

    state.phone = data.phone
    state.displayName = data.name
    state.role = data.role

    saveToStorage()
    hideLogin()
    showOwnerChat()
  } catch (err) {
    console.error('[owner] login failed:', err)
    const errEl = document.getElementById('owner-login-error')
    const msg = err.code === 'TIMEOUT'
      ? 'Request timed out. Please check your connection and try again.'
      : 'Network error. Please check your connection and try again.'
    if (errEl) { errEl.textContent = msg; errEl.classList.remove('hidden') }
  } finally {
    setButtonLoading(btn, false)
  }
}

function showOwnerChat() {
  connection.disconnect()

  const chatEl = document.getElementById('owner-chat')
  if (!chatEl) {
    console.error('[owner] owner-chat element not found')
    return
  }

  document.getElementById('page-root')?.classList.add('hidden')
  chatEl.classList.remove('hidden')

  const badge = document.getElementById('owner-role-badge')
  if (badge) badge.textContent = state.role

  ownerChat.state.phone = state.phone
  ownerChat.state.role = state.role
  ownerChat.connectOwnerWs()
  setupOwnerForm()
}

function backToPublic() {
  ownerChat.disconnectOwnerWs()
  teardownOwnerForm()
  clearFromStorage()

  document.getElementById('owner-chat')?.classList.add('hidden')
  document.getElementById('page-root')?.classList.remove('hidden')

  const container = document.getElementById('chat-messages')
  if (container) container.innerHTML = '<p id="fallback-hint" class="text-xs text-center" style="color: var(--color-text-weak);">Ask AI a question...</p>'
  const attachContainer = document.getElementById('owner-attachments')
  if (attachContainer) attachContainer.innerHTML = ''
  uploadState.pending = []
}
