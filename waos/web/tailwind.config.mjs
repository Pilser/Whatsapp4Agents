/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,ts,tsx}'],
  theme: {
    extend: {
      colors: {
        waos: {
          bg: '#111b21',
          surface: '#1f2c33',
          text: '#e9edef',
          muted: '#8696a0',
          dim: '#667781',
          accent: '#25D366',
          'accent-hover': '#20bd5a',
          border: '#313d45',
          danger: '#ef5350',
          warn: '#ffa000',
          info: '#53bdeb',
          cream: '#d9dbd1',
          'cream-bg': '#efeae2',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Fira Code', 'monospace'],
      },
    },
  },
  plugins: [],
};
