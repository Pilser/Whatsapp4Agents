import { els } from './dom.js'

const STORAGE_PHONE = 'zerowrapper_phone'
const STORAGE_NAME = 'zerowrapper_name'
const STORAGE_SESSION = 'zerowrapper_session'

export const state = {
  phone: '',
  displayName: '',
  sessionId: '',
}

export function loadFromStorage() {
  state.phone = localStorage.getItem(STORAGE_PHONE) || ''
  state.displayName = localStorage.getItem(STORAGE_NAME) || ''
  state.sessionId = localStorage.getItem(STORAGE_SESSION) || ''
}

export function saveToStorage() {
  if (state.phone) localStorage.setItem(STORAGE_PHONE, state.phone)
  if (state.displayName) localStorage.setItem(STORAGE_NAME, state.displayName)
  if (state.sessionId) localStorage.setItem(STORAGE_SESSION, state.sessionId)
}

export function clearFromStorage() {
  localStorage.removeItem(STORAGE_PHONE)
  localStorage.removeItem(STORAGE_NAME)
  localStorage.removeItem(STORAGE_SESSION)
}

export function normalizePhone(code, num) {
  num = num.replace(/^0+/, '')
  code = code.replace(/^\+/, '')
  return code + num
}

export function showForm() {
  els.authSection.classList.remove('hidden')
  els.userBadge.classList.add('hidden')
}

export function showBadge() {
  els.authSection.classList.add('hidden')
  els.userBadge.classList.remove('hidden')
  const name = state.displayName || state.phone
  els.badgeDisplay.textContent = state.phone + (name && name !== state.phone ? ' (' + name + ')' : '')
}

export function scrollToAuth() {
  els.authSection.scrollIntoView({ behavior: 'smooth', block: 'center' })
  els.phoneInput.focus()
  els.authSection.style.transition = 'box-shadow 0.3s'
  els.authSection.style.boxShadow = '0 0 0 3px var(--color-bg-interactive)'
  setTimeout(() => { els.authSection.style.boxShadow = 'none' }, 2000)
}

export function showAuthError(msg) {
  els.authError.textContent = msg
  els.authError.classList.remove('hidden')
}

export function hideAuthError() {
  els.authError.classList.add('hidden')
}
