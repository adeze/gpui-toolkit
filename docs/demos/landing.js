'use strict';
const input = document.querySelector('#search');
const cards = [...document.querySelectorAll('.card')];
const count = document.querySelector('#count');
function update() {
  const q = input.value.toLowerCase().trim();
  let shown = 0;
  for (const card of cards) {
    card.hidden = q !== '' && !card.dataset.search.includes(q);
    if (!card.hidden) shown++;
  }
  count.textContent = `${shown} demos`;
}
input.addEventListener('input', update);
update();
const tabs = [...document.querySelectorAll('[role=tab]')];
function selectTab(tab) {
  for (const item of tabs) {
    const selected = item === tab;
    item.setAttribute('aria-selected', String(selected));
    item.tabIndex = selected ? 0 : -1;
    document.getElementById(item.getAttribute('aria-controls')).hidden = !selected;
  }
}
for (const [index, tab] of tabs.entries()) {
  tab.addEventListener('click', () => selectTab(tab));
  tab.addEventListener('keydown', event => {
    let next;
    if (event.key === 'ArrowRight') next = tabs[(index + 1) % tabs.length];
    if (event.key === 'ArrowLeft') next = tabs[(index + tabs.length - 1) % tabs.length];
    if (event.key === 'Home') next = tabs[0];
    if (event.key === 'End') next = tabs[tabs.length - 1];
    if (next) { event.preventDefault(); selectTab(next); next.focus(); }
  });
}
