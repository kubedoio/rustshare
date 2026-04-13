/** @type {import('tailwindcss').Config} */
export default {
	content: ['./src/**/*.{html,js,svelte,ts}'],
	theme: {
		extend: {
			colors: {
				brand: {
					50: '#fff4ec',
					100: '#f8e2d2',
					200: '#efc4a8',
					300: '#e29b74',
					400: '#d47648',
					500: '#c65a1e',
					600: '#a34716',
					700: '#833a15',
					800: '#683016',
					900: '#562916',
					950: '#2e1409'
				}
			},
			fontFamily: {
				sans: ['Instrument Sans', 'system-ui', 'sans-serif'],
				display: ['Fraunces', 'Georgia', 'serif'],
				data: ['IBM Plex Sans', 'system-ui', 'sans-serif'],
				mono: ['IBM Plex Mono', 'monospace']
			},
			fontSize: {
				'2xs': ['0.625rem', { lineHeight: '0.875rem' }],
				'meta': ['0.75rem', { lineHeight: '1.35', fontWeight: '500' }],
				'body-sm': ['0.875rem', { lineHeight: '1.45', fontWeight: '400' }]
			},
			boxShadow: {
				panel: '0 18px 45px rgba(39, 25, 14, 0.08)',
				'panel-dark': '0 22px 54px rgba(0, 0, 0, 0.24)'
			},
			borderRadius: {
				// Design system hierarchy: sm(6px) md(10px) lg(14px) xl(20px) pill(999px)
				'sm': '0.375rem',    // 6px
				'md': '0.625rem',    // 10px
				'lg': '0.875rem',    // 14px
				'xl': '1.25rem',     // 20px
				'2xl': '1.5rem',     // 24px - for cards/panels
				'3xl': '2rem'        // 32px - for large containers
			}
		}
	},
	plugins: [require('daisyui')],
	daisyui: {
		themes: [
			{
				'rustshare-light': {
					'primary': '#c65a1e',
					'primary-content': '#fff8f3',
					'secondary': '#7b4a2e',
					'secondary-content': '#fff8f3',
					'accent': '#b87542',
					'accent-content': '#fff8f3',
					'neutral': '#151515',
					'neutral-content': '#f3efe8',
					'base-100': '#fbf9f5',
					'base-200': '#f6f3ee',
					'base-300': '#ded6ca',
					'base-content': '#151515',
					'info': '#366d8c',
					'success': '#1d7a52',
					'warning': '#a56a12',
					'error': '#b63e3e',
					'--rounded-btn': '0.625rem',
					'--rounded-box': '1.25rem',
					'--rounded-badge': '999px',
				}
			},
			{
				'rustshare-dark': {
					'primary': '#c46a35',
					'primary-content': '#ffffff',
					'secondary': '#b28366',
					'secondary-content': '#fff7f1',
					'accent': '#7b4a2e',
					'accent-content': '#fff7f1',
					'neutral': '#1b1815',
					'neutral-content': '#f3efe8',
					'base-100': '#121315',
					'base-200': '#181a1d',
					'base-300': '#24272c',
					'base-content': '#f3efe8',
					'info': '#5a8fae',
					'success': '#3aa06f',
					'warning': '#cf9129',
					'error': '#d25a5a',
					'--rounded-btn': '0.625rem',
					'--rounded-box': '1.25rem',
					'--rounded-badge': '999px',
				}
			}
		],
		darkTheme: 'rustshare-dark',
		base: true,
		styled: true,
		utils: true,
	}
};
