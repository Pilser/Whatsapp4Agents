import hljs from 'highlight.js'

export function highlight(el) {
  el.querySelectorAll('pre code').forEach((block) => {
    hljs.highlightElement(block)
  })
}

export function wrapTables(el) {
  el.querySelectorAll('table').forEach((t) => {
    if (t.parentElement && t.parentElement.classList.contains('table-wrap')) return
    const outer = document.createElement('div')
    outer.className = 'table-outer-wrap'
    const wrapper = document.createElement('div')
    wrapper.className = 'table-wrap'
    t.parentNode.insertBefore(outer, t)
    outer.appendChild(wrapper)
    wrapper.appendChild(t)

    const fade = document.createElement('div')
    fade.className = 'scroll-fade'
    outer.appendChild(fade)
    const arrow = document.createElement('div')
    arrow.className = 'scroll-arrow'
    arrow.textContent = '\u2192'
    outer.appendChild(arrow)

    let scrollTimer
    updateOverflow()
    wrapper.addEventListener('scroll', () => {
      outer.classList.add('scrolling')
      clearTimeout(scrollTimer)
      checkScrollEnd()
    })
    const ro = new ResizeObserver(updateOverflow)
    ro.observe(wrapper)

    function checkScrollEnd() {
      clearTimeout(scrollTimer)
      scrollTimer = setTimeout(() => {
        outer.classList.remove('scrolling')
        updateOverflow()
      }, 350)
    }
    function updateOverflow() {
      if (wrapper.scrollWidth > wrapper.clientWidth) {
        outer.classList.add('overflow')
        const atEnd = wrapper.scrollLeft + wrapper.clientWidth >= wrapper.scrollWidth - 1
        outer.classList.toggle('scrolled-end', atEnd)
      } else {
        outer.classList.remove('overflow', 'scrolled-end', 'scrolling')
      }
    }
  })
}

export function render(el, html) {
  el.innerHTML = html
  wrapTables(el)
  highlight(el)
}
