import { mount } from 'svelte'
import './app.css'
import Settings from './settings/Settings.svelte'

const target = document.getElementById('app')
if (!target) throw new Error('#app 엘리먼트를 찾을 수 없습니다')

export default mount(Settings, { target })
