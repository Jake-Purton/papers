import { api, store, progress, getTheme, setTheme, esc, formatSize, relativeTime, coverHTML, icons } from './common.js'

const $ = s => document.querySelector(s)

const state = {
    books: null,
    error: null,
    q: new URLSearchParams(location.search).get('q') ?? '',
    format: store.get('format', 'all'),
    sort: store.get('sort', 'recent'),
    view: store.get('view', 'list'),
}

// ---------- search ----------

const fold = s => (s ?? '').normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()

const tokens = q => fold(q).split(/\s+/).filter(Boolean)

function matches(book, terms) {
    if (!terms.length) return true
    const hay = fold(`${book.title} ${book.author ?? ''} ${book.filename}`)
    return terms.every(t => hay.includes(t))
}

function highlight(text, terms) {
    if (!text) return ''
    if (!terms.length) return esc(text)
    const re = new RegExp(`(${terms.map(t => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'gi')
    return text.split(re).map((part, i) => i % 2 ? `<mark>${esc(part)}</mark>` : esc(part)).join('')
}

// ---------- sorting & grouping ----------

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

const sorters = {
    recent: (a, b) => (b.accessed_at ?? -1) - (a.accessed_at ?? -1) || (b.added_at ?? 0) - (a.added_at ?? 0),
    added: (a, b) => (b.added_at ?? 0) - (a.added_at ?? 0) || b.id - a.id,
    title: (a, b) => collator.compare(a.title, b.title),
    author: (a, b) => (!a.author - !b.author) || collator.compare(a.author ?? '', b.author ?? '') || collator.compare(a.title, b.title),
}

function bucket(seconds) {
    if (!seconds) return 'Not opened yet'
    const midnight = new Date().setHours(0, 0, 0, 0) / 1000
    if (seconds >= midnight) return 'Today'
    if (seconds >= midnight - 86400) return 'Yesterday'
    if (seconds >= midnight - 86400 * 6) return 'This week'
    if (seconds >= midnight - 86400 * 30) return 'This month'
    return 'Earlier'
}

function groups(books) {
    if (state.sort !== 'recent') return [[null, books]]
    const out = new Map()
    for (const b of books) {
        const key = bucket(b.accessed_at)
        if (!out.has(key)) out.set(key, [])
        out.get(key).push(b)
    }
    return [...out]
}

// ---------- rendering ----------

const readerUrl = book => `reader.html?id=${book.id}`

function when(book) {
    return book.accessed_at
        ? `<span title="Last opened">${esc(relativeTime(book.accessed_at))}</span>`
        : `<span title="Added">Added ${esc(relativeTime(book.added_at))}</span>`
}

function progressBar(book) {
    const fraction = progress.get(book.id)?.fraction
    if (!fraction || fraction < 0.01) return ''
    const pct = Math.round(fraction * 100)
    return `<div class="progress" title="${pct}% read"><span style="width:${pct}%"></span></div>`
}

const downloadLink = book =>
    `<a class="icon-button download" href="${api.downloadUrl(book.id)}" download aria-label="Download ${esc(book.title)}" title="Download">${icons.download}</a>`

function row(book, terms) {
    const author = book.author ? `<span class="author">${highlight(book.author, terms)}</span><span class="sep">·</span>` : ''
    return `<div class="row">
        ${coverHTML(book, { small: true })}
        <div class="row-main">
            <a class="row-title stretched" href="${readerUrl(book)}" title="${esc(book.title)}">${highlight(book.title, terms)}</a>
            <div class="row-sub">${author}<span class="file">${highlight(book.filename, terms)}</span></div>
        </div>
        <div class="row-meta">
            ${progressBar(book)}
            <span class="badge ${esc(book.format)}">${esc(book.format)}</span>
            <span class="size">${esc(formatSize(book.size))}</span>
            <span class="when">${when(book)}</span>
            ${downloadLink(book)}
        </div>
    </div>`
}

function card(book, terms) {
    return `<div class="card">
        ${coverHTML(book)}
        ${downloadLink(book)}
        ${progressBar(book)}
        <div>
            <a class="card-title stretched" href="${readerUrl(book)}" title="${esc(book.title)}">${highlight(book.title, terms)}</a>
            <div class="card-sub">
                <span class="badge ${esc(book.format)}">${esc(book.format)}</span>
                <span class="author">${book.author ? highlight(book.author, terms) : when(book)}</span>
            </div>
        </div>
    </div>`
}

function empty(glyph, title, body, button) {
    return `<div class="empty">
        <div class="glyph" aria-hidden="true">${glyph}</div>
        <h2>${title}</h2>
        <p>${body}</p>
        ${button ?? ''}
    </div>`
}

function render() {
    const results = $('#results')

    $('#view-list').setAttribute('aria-pressed', state.view === 'list')
    $('#view-grid').setAttribute('aria-pressed', state.view === 'grid')
    $('#sort').value = state.sort

    if (state.error) {
        results.innerHTML = empty('?', 'Couldn’t reach the library', esc(state.error), '<button type="button" data-action="retry">Try again</button>')
        $('#chips').innerHTML = ''
        $('#foot').textContent = ''
        return
    }
    if (!state.books) return

    const all = state.books
    const terms = tokens(state.q)
    const found = all.filter(b => matches(b, terms))
    const shown = found.filter(b => state.format === 'all' || b.format === state.format).sort(sorters[state.sort])

    $('#count').textContent = all.length ? `${all.length}` : ''

    const counts = { all: found.length, pdf: 0, epub: 0 }
    for (const b of found) counts[b.format] = (counts[b.format] ?? 0) + 1
    $('#chips').innerHTML = [['all', 'All'], ['pdf', 'PDF'], ['epub', 'EPUB']]
        .map(([key, label]) => `<button class="chip" type="button" data-format="${key}" aria-pressed="${state.format === key}">${label}<span class="n">${counts[key]}</span></button>`)
        .join('')

    if (!all.length) {
        results.innerHTML = empty('❦', 'Your library is empty', 'Drop PDF or EPUB files into the books folder (<code>data/books</code> by default) and they’ll appear here.')
    } else if (!shown.length) {
        results.innerHTML = empty('∅', state.q ? `No matches for “${esc(state.q)}”` : 'Nothing here', state.q ? 'Try fewer words, or search by author or file name.' : `No ${esc(state.format.toUpperCase())} files yet.`, state.q ? '<button type="button" data-action="clear">Clear search</button>' : '')
    } else {
        const item = state.view === 'grid' ? card : row
        results.innerHTML = groups(shown).map(([label, books]) => `
            ${label ? `<h2 class="section-head">${esc(label)} <span class="n">${books.length}</span></h2>` : ''}
            <div class="${state.view}">${books.map(b => item(b, terms)).join('')}</div>`).join('')
    }

    const total = shown.reduce((n, b) => n + (b.size || 0), 0)
    $('#foot').textContent = shown.length ? `${shown.length} ${shown.length === 1 ? 'item' : 'items'} · ${formatSize(total)}` : ''
}

// ---------- data ----------

async function load() {
    try {
        state.books = await api.books()
        state.error = null
    } catch (e) {
        // keep showing what we had if a background refresh fails
        if (!state.books) state.error = e.message
    }
    render()
}

// ---------- controls ----------

const themeIcons = { auto: icons.auto, light: icons.sun, dark: icons.moon }
const themeNext = { auto: 'light', light: 'dark', dark: 'auto' }

function renderTheme() {
    // sepia is reader-only, the library shows it as light
    const theme = getTheme() === 'sepia' ? 'light' : getTheme()
    $('#theme').innerHTML = themeIcons[theme]
    $('#theme').title = `Theme: ${theme}`
    $('#theme').setAttribute('aria-label', `Theme: ${theme}`)
}

$('#search-icon').outerHTML = icons.search
$('#clear').innerHTML = icons.x
$('#view-list').innerHTML = icons.list
$('#view-grid').innerHTML = icons.grid
renderTheme()

const q = $('#q')
q.value = state.q

function setQuery(value) {
    state.q = value
    const url = new URL(location.href)
    if (value) url.searchParams.set('q', value)
    else url.searchParams.delete('q')
    history.replaceState(null, '', url)
    render()
}

q.addEventListener('input', () => setQuery(q.value))

q.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
        if (q.value) { q.value = ''; setQuery('') } else q.blur()
    } else if (e.key === 'Enter') {
        $('#results .stretched')?.click()
    }
})

$('#clear').addEventListener('click', () => { q.value = ''; setQuery(''); q.focus() })

document.addEventListener('keydown', e => {
    const typing = e.target.closest('input, select, textarea, [contenteditable]')
    if (e.key === '/' && !typing && !e.metaKey && !e.ctrlKey) {
        e.preventDefault()
        q.focus()
        q.select()
    }
})

$('#chips').addEventListener('click', e => {
    const chip = e.target.closest('[data-format]')
    if (!chip) return
    state.format = chip.dataset.format
    store.set('format', state.format)
    render()
})

$('#sort').addEventListener('change', e => {
    state.sort = e.target.value
    store.set('sort', state.sort)
    render()
})

for (const view of ['list', 'grid']) {
    $(`#view-${view}`).addEventListener('click', () => {
        state.view = view
        store.set('view', view)
        render()
    })
}

$('#theme').addEventListener('click', () => {
    const current = getTheme() === 'sepia' ? 'light' : getTheme()
    setTheme(themeNext[current])
    renderTheme()
})

$('#results').addEventListener('click', e => {
    const action = e.target.closest('[data-action]')?.dataset.action
    if (action === 'retry') { state.error = null; render(); load() }
    if (action === 'clear') { q.value = ''; setQuery(''); q.focus() }
})

// access times change while reading and the watcher may add files, refresh on return
addEventListener('pageshow', e => { if (e.persisted) load() })
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') load() })

load()
