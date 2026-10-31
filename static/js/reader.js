import { api, store, progress, getTheme, setTheme, esc, icons } from './common.js'

const $ = s => document.querySelector(s)

const renderers = {
    pdf: () => import('./renderers/pdf.js'),
    epub: () => import('./renderers/epub.js'),
}

const id = new URLSearchParams(location.search).get('id')
if (!id) location.replace('./')

const settings = {
    fontSize: store.get('fontSize', 100),
    flow: store.get('flow', 'paginated'),
}

let reader = null

// ---------- chrome ----------

$('#back').insertAdjacentHTML('afterbegin', icons.back)
$('#toc-button').innerHTML = icons.toc
$('#type-button').innerHTML = icons.type
$('#download').innerHTML = icons.download
$('#fullscreen').innerHTML = icons.expand
$('#drawer-close').innerHTML = icons.x

// going back to the library keeps its search and scroll position
$('#back').addEventListener('click', e => {
    const ref = document.referrer && new URL(document.referrer)
    if (ref && ref.origin === location.origin && (ref.pathname === '/' || ref.pathname.endsWith('/index.html'))) {
        e.preventDefault()
        history.back()
    }
})

function status(html) {
    $('#stage').innerHTML = `<div class="status">${html}</div>`
}

// ---------- appearance ----------

function appearance() {
    const style = getComputedStyle(document.documentElement)
    const v = name => style.getPropertyValue(name).trim()
    return {
        page: v('--page'),
        ink: v('--page-ink'),
        accent: v('--accent'),
        scheme: v('color-scheme') || 'light',
        fontSize: settings.fontSize,
        flow: settings.flow,
    }
}

function renderSettings() {
    const theme = getTheme()
    for (const b of document.querySelectorAll('#themes button')) b.setAttribute('aria-pressed', b.dataset.theme === theme)
    for (const b of document.querySelectorAll('#flows button')) b.setAttribute('aria-pressed', b.dataset.flow === settings.flow)
    $('#size').textContent = `${settings.fontSize}%`
    $('#smaller').disabled = settings.fontSize <= 70
    $('#larger').disabled = settings.fontSize >= 200
}

function applyAppearance() {
    renderSettings()
    reader?.setAppearance?.(appearance())
}

$('#themes').addEventListener('click', e => {
    const b = e.target.closest('[data-theme]')
    if (!b) return
    setTheme(b.dataset.theme)
    applyAppearance()
})

$('#flows').addEventListener('click', e => {
    const b = e.target.closest('[data-flow]')
    if (!b) return
    settings.flow = b.dataset.flow
    store.set('flow', settings.flow)
    applyAppearance()
})

function resize(delta) {
    settings.fontSize = Math.min(200, Math.max(70, settings.fontSize + delta))
    store.set('fontSize', settings.fontSize)
    applyAppearance()
}
$('#smaller').addEventListener('click', () => resize(-10))
$('#larger').addEventListener('click', () => resize(10))

// follow the os when on auto
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (getTheme() === 'auto') applyAppearance()
})

// ---------- popover ----------

const pop = $('#type-pop')

function togglePopover(open = pop.hidden) {
    pop.hidden = !open
    $('#type-button').setAttribute('aria-expanded', open)
}

$('#type-button').addEventListener('click', e => { e.stopPropagation(); togglePopover() })
document.addEventListener('click', e => { if (!pop.hidden && !e.target.closest('.pop-wrap')) togglePopover(false) })

// ---------- contents drawer ----------

function tocHTML(items, depth = 0) {
    return `<ol>${items.map(item => `<li>
        <a href="#" data-href="${esc(item.href)}" style="--d:${depth}">${esc(item.label)}</a>
        ${item.subitems?.length ? tocHTML(item.subitems, depth + 1) : ''}
    </li>`).join('')}</ol>`
}

function toggleDrawer(open = !$('#drawer').classList.contains('open')) {
    $('#drawer').classList.toggle('open', open)
    $('#scrim').classList.toggle('open', open)
    if (open) ($('#toc a[aria-current="true"]') ?? $('#toc a'))?.focus({ preventScroll: false })
}

$('#toc-button').addEventListener('click', () => toggleDrawer(true))
$('#drawer-close').addEventListener('click', () => toggleDrawer(false))
$('#scrim').addEventListener('click', () => toggleDrawer(false))
$('#toc').addEventListener('click', e => {
    const a = e.target.closest('a[data-href]')
    if (!a) return
    e.preventDefault()
    reader?.goTo(a.dataset.href)
    toggleDrawer(false)
})

// ---------- fullscreen ----------

if (!document.fullscreenEnabled) $('#fullscreen').hidden = true

function toggleFullscreen() {
    if (document.fullscreenElement) document.exitFullscreen()
    else document.documentElement.requestFullscreen?.()
}
$('#fullscreen').addEventListener('click', toggleFullscreen)

// ---------- keyboard ----------

function onKeydown(e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return
    if (e.target.closest?.('input, select, textarea')) return

    if (e.key === 'Escape') {
        togglePopover(false)
        toggleDrawer(false)
        return
    }
    if (e.key === 'f') return toggleFullscreen()
    if (!reader?.navigation) return

    switch (e.key) {
        case 'ArrowLeft': reader.left(); break
        case 'ArrowRight': reader.right(); break
        case 'PageUp': reader.prev(); break
        case 'PageDown': reader.next(); break
        case ' ': e.shiftKey ? reader.prev() : reader.next(); break
        case 't': toggleDrawer(); break
        default: return
    }
    e.preventDefault()
}
document.addEventListener('keydown', onKeydown)

// ---------- position ----------

let saveTimer = null
let dragging = false

function onRelocate({ location, fraction, label, href }) {
    const pct = Math.round((fraction ?? 0) * 100)
    $('#chapter').textContent = label ?? ''
    $('#pct').textContent = `${pct}%`
    if (!dragging) {
        $('#slider').value = fraction ?? 0
        $('#slider').style.setProperty('--p', `${pct}%`)
    }

    for (const a of document.querySelectorAll('#toc a')) a.setAttribute('aria-current', a.dataset.href === href)

    clearTimeout(saveTimer)
    saveTimer = setTimeout(() => progress.set(id, { location, fraction }), 400)
}

const slider = $('#slider')
slider.addEventListener('input', () => {
    dragging = true
    const pct = Math.round(slider.value * 100)
    $('#pct').textContent = `${pct}%`
    slider.style.setProperty('--p', `${pct}%`)
})
slider.addEventListener('change', () => {
    dragging = false
    reader?.goToFraction(parseFloat(slider.value))
})

// ---------- open ----------

async function main() {
    let book
    try {
        book = await api.book(id)
    } catch (e) {
        document.title = 'Not found · Papers'
        status(e.status === 404
            ? `<h2>This book isn’t in the library</h2><p>It may have been moved or deleted.</p><a class="button" href="./">${icons.back} Back to library</a>`
            : `<h2>Couldn’t reach the library</h2><p>${esc(e.message)}</p>`)
        return
    }

    document.title = `${book.title} · Papers`
    $('#title').textContent = book.title
    $('#title').title = book.title
    $('#author').textContent = book.author ?? ''

    const download = $('#download')
    download.href = api.downloadUrl(book.id)
    download.setAttribute('download', book.filename)

    const load = renderers[book.format]
    if (!load) {
        status(`<h2>Unsupported format</h2><p>${esc(book.format)} files can’t be opened here.</p>`)
        return
    }

    try {
        const { open } = await load()
        const stage = $('#stage')
        stage.replaceChildren()
        reader = await open(stage, book, {
            location: progress.get(book.id)?.location,
            appearance: appearance(),
            onRelocate,
            onKeydown,
        })
    } catch (e) {
        console.error(e)
        status(`<h2>Couldn’t open this book</h2><p>The file may be damaged or use features the reader doesn’t support.</p>
            <a class="button" href="${api.downloadUrl(book.id)}" download>${icons.download} Download instead</a>`)
        return
    }

    if (reader.navigation) {
        $('#foot').hidden = false
        const stage = $('#stage')
        stage.insertAdjacentHTML('beforeend', `
            <button class="edge left" type="button" aria-label="Previous page">${icons.left}</button>
            <button class="edge right" type="button" aria-label="Next page">${icons.right}</button>`)
        stage.querySelector('.edge.left').addEventListener('click', () => reader.left())
        stage.querySelector('.edge.right').addEventListener('click', () => reader.right())
    }

    if (reader.appearance) {
        $('#type-button').hidden = false
        renderSettings()
    }

    if (reader.toc.length) {
        $('#toc-button').hidden = false
        $('#toc').innerHTML = tocHTML(reader.toc)
    } else {
        $('#toc').innerHTML = '<p class="none">This book has no table of contents.</p>'
    }
}

if (id) main()
