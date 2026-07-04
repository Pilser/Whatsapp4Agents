let lightbox = null
let loadingEl = null

function ensureLightbox() {
  if (!lightbox) {
    lightbox = document.createElement('div')
    lightbox.id = 'lightbox'
    lightbox.innerHTML = '<button class="lb-close">&times;</button>'
    loadingEl = document.createElement('div')
    loadingEl.className = 'lb-loading'
    loadingEl.textContent = 'Loading...'
    lightbox.appendChild(loadingEl)
    document.body.appendChild(lightbox)
    lightbox.addEventListener('click', (e) => {
      if (e.target === lightbox || e.target.classList.contains('lb-close')) close()
    })
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') close()
    })
  }
  return lightbox
}

function close() {
  if (lightbox) {
    lightbox.classList.remove('active')
    document.body.style.overflow = ''
    const img = lightbox.querySelector('img')
    if (img) { img.onload = null; img.onerror = null }
  }
}

function open(src) {
  const lb = ensureLightbox()
  let img = lb.querySelector('img')
  if (!img) {
    img = document.createElement('img')
    img.style.display = 'none'
    lb.appendChild(img)
  } else {
    img.style.display = 'none'
    img.onload = null
    img.onerror = null
  }
  if (loadingEl) loadingEl.style.display = 'block'
  lb.classList.add('active')
  document.body.style.overflow = 'hidden'

  img.onload = () => {
    img.style.display = 'block'
    if (loadingEl) loadingEl.style.display = 'none'
  }
  img.onerror = () => {
    if (loadingEl) loadingEl.textContent = 'Failed to load image'
    setTimeout(() => { if (loadingEl) loadingEl.textContent = 'Loading...' }, 2000)
  }
  img.src = src
}

export function init(messagesEl) {
  messagesEl.addEventListener('click', (e) => {
    if (e.target.tagName === 'IMG' && e.target.closest('.chat-markdown')) {
      open(e.target.src)
    }
  })
}
