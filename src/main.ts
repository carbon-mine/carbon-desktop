import { mount } from 'svelte'
import App from './App.svelte'

if (/Windows/.test(navigator.userAgent)) document.documentElement.dataset.desktopPlatform = 'windows'
mount(App, { target: document.querySelector('#app')! })
