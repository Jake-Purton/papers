// loaded as a classic script in <head> so a saved theme is applied before first paint
try {
    const theme = localStorage.getItem('papers:theme')
    if (theme && theme !== 'auto') document.documentElement.dataset.theme = theme
} catch {}
