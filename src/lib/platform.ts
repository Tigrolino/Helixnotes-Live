import type { CustomTheme } from '$lib/types';

// Platform is the compile-time build target, injected by the backend into
// window.__HELIX_PLATFORM__ before app scripts run (see src-tauri/src/lib.rs). The UA is only a
// fallback when that global is absent (SSR/prerender, plain-browser dev): some desktop WebKitGTK
// builds report a mobile-looking user-agent. (#63)
type HelixPlatform = { mobile: boolean; android: boolean; ios: boolean };

const injected: HelixPlatform | undefined =
	typeof window !== 'undefined'
		? (window as unknown as { __HELIX_PLATFORM__?: HelixPlatform }).__HELIX_PLATFORM__
		: undefined;

const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';

export const isAndroid = injected ? injected.android : /android/i.test(ua);
export const isIOS = injected ? injected.ios : /iphone|ipad|ipod/i.test(ua);
export const isMobile = injected ? injected.mobile : isAndroid || isIOS;

// Themes that use the dark color scheme. Used by applyTheme() to toggle the
// `dark` class on the root element. Keep this in sync when adding new themes.
export const darkThemes = [
	'dark',
	'solarized-dark',
	'catppuccin',
	'nord',
	'tokyo-night',
	'github-dark',
	'dracula',
	'blueberry',
	'forest-green',
	'gruvbox',
	'midnight-tide',
	'cherry-blossom',
	'synthwave',
	'ember',
	'moonlit',
	'dark-coffee',
	'crimson',
	'material-dark',
	'monokai',
	'rose-pine',
	'everforest',
	'horizon',
	'cyberpunk',
	'black',
	'one-dark',
];

export function isDarkTheme(theme: string): boolean {
	return darkThemes.includes(theme) || (theme === 'system' && typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches);
}

export const namedThemes = [
	'solarized-light', 'solarized-dark', 'catppuccin', 'nord', 'tokyo-night',
	'github-light', 'github-dark', 'dracula', 'blueberry', 'forest-green',
	'gruvbox', 'midnight-tide', 'cherry-blossom', 'synthwave', 'ember',
	'moonlit', 'light-coffee', 'dark-coffee', 'cotton-candy', 'crimson',
	'cloud', 'peach', 'material-dark', 'material-light', 'monokai',
	'rose-pine', 'everforest', 'horizon', 'cyberpunk', 'black', 'one-dark',
];

const CUSTOM_THEME_VARS = [
	'--bg-primary', '--bg-secondary', '--bg-tertiary',
	'--bg-hover', '--bg-active', '--bg-editor',
	'--text-primary', '--text-secondary', '--border-color',
];

function applyCustomThemeVars(root: HTMLElement, ct: CustomTheme) {
	root.style.setProperty('--bg-primary', ct.colors.bg_primary);
	root.style.setProperty('--bg-secondary', ct.colors.bg_secondary);
	root.style.setProperty('--bg-tertiary', ct.colors.bg_tertiary);
	root.style.setProperty('--bg-hover', ct.colors.bg_hover);
	root.style.setProperty('--bg-active', ct.colors.bg_active);
	root.style.setProperty('--bg-editor', ct.colors.bg_editor);
	root.style.setProperty('--text-primary', ct.colors.text_primary);
	root.style.setProperty('--text-secondary', ct.colors.text_secondary);
	root.style.setProperty('--border-color', ct.colors.border_color);
}

function clearCustomThemeVars(root: HTMLElement) {
	for (const v of CUSTOM_THEME_VARS) root.style.removeProperty(v);
}

export function applyTheme(t: string, themes: CustomTheme[] = []) {
	if (typeof document === 'undefined') return;
	const root = document.documentElement;
	root.classList.remove('dark');
	root.removeAttribute('data-theme');
	clearCustomThemeVars(root);
	if (t.startsWith('custom-')) {
		const ct = themes.find(c => c.id === t);
		if (ct) {
			applyCustomThemeVars(root, ct);
			if (ct.is_dark) root.classList.add('dark');
		}
	} else if (namedThemes.includes(t)) {
		root.setAttribute('data-theme', t);
		if (darkThemes.includes(t)) root.classList.add('dark');
	} else if (t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)) {
		root.classList.add('dark');
	}
}
