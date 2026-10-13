/* Browser counterpart of MiniApp's native menu. Query changes reboot the demo. */
(() => {
  'use strict';
  const url = new URL(location.href);
  // Pixel parity captures measure the application viewport, without web chrome.
  // Normal gallery captures retain the menu, so its layout is also exercised.
  if (url.searchParams.get('qa') === '1') return;
  const themes = ['dark', 'light', 'midnight', 'forest', 'black-and-white', 'onyx', 'carbon-white', 'carbon-gray-10', 'carbon-gray-90', 'carbon-gray-100'];
  const designs = ['neutral', 'apple', 'apple_hig', 'material3', 'fluent', 'adwaita', 'breeze', 'carbon'];
  const names = { apple: 'macOS', apple_hig: 'Apple HIG', material3: 'Material 3', fluent: 'Fluent', neutral: 'Neutral', adwaita: 'Adwaita', breeze: 'Breeze', carbon: 'Carbon' };
  const demos = { showcase: 'UI Kit', px: 'Charts', d3rs: 'D3 primitives', spinorama: 'Spinorama', hello: 'Hello WebGPU' };
  const app = document.documentElement.dataset.demo;
  const style = document.createElement('style');
  style.textContent = `
    body { display:flex; flex-direction:column; overflow:hidden; }
    body > canvas { height:0!important; min-height:0; flex:1 1 0; width:100%; }
    #demo-menu { flex:none; display:flex; align-items:center; gap:20px; padding:12px 20px; background:#202923; color:#e8ede5; border-bottom:1px solid #526048; font:12px/1.4 system-ui,sans-serif; z-index:10; }
    #demo-menu a { color:inherit; text-decoration:none; font-weight:650; white-space:nowrap; }
    #demo-menu a:first-child { margin-right:auto; }
    #demo-menu label { display:flex; align-items:center; gap:8px; }
    #demo-menu select { background:#303d32; color:#e8ede5; border:1px solid #627359; border-radius:4px; padding:6px; font:inherit; max-width:150px; }
    #demo-menu a:focus-visible,#demo-menu select:focus-visible { outline:2px solid #d4fa72; outline-offset:3px; }
    #demo-menu small { color:#b6c7ac; }
    @media(max-width:650px) { #demo-menu { flex-wrap:wrap; gap:8px 12px; padding:8px 12px; } #demo-menu a:first-child { width:100%; } #demo-menu label { gap:4px; } #demo-menu select { max-width:115px; } #demo-menu small { display:none; } }
  `;
  document.head.append(style);
  const menu = document.createElement('nav');
  menu.id = 'demo-menu';
  menu.setAttribute('aria-label', 'Demo controls');
  const home = document.createElement('a');
  home.href = '/'; home.textContent = '← GPUI Toolkit'; menu.append(home);
  function selector(label, key, values, initial, onChange) {
    const container = document.createElement('label');
    container.textContent = label;
    const select = document.createElement('select');
    select.name = key;
    for (const value of values) {
      const option = document.createElement('option');
      option.value = value; option.textContent = names[value] || demos[value] || value.replaceAll('-', ' ');
      select.append(option);
    }
    select.value = values.includes(initial) ? initial : values[0];
    select.addEventListener('change', () => onChange(select.value));
    container.append(select); menu.append(container);
  }
  selector('Demo', 'demo', Object.keys(demos), app, value => {
    const destination = new URL(`/${value}/`, location.origin);
    for (const key of ['theme', 'style']) if (url.searchParams.has(key)) destination.searchParams.set(key, url.searchParams.get(key));
    location.assign(destination);
  });
  const hint = document.createElement('small');
  hint.textContent = 'Appearance changes restart the demo';
  selector('Theme', 'theme', themes, url.searchParams.get('theme') || 'dark', value => {
    url.searchParams.set('theme', value); location.assign(url);
  });
  selector('Design', 'style', designs, url.searchParams.get('style') || 'neutral', value => {
    url.searchParams.set('style', value); location.assign(url);
  });
  menu.append(hint);
  document.body.prepend(menu);
})();
