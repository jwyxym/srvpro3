const resize = () => {
	document.documentElement.style.setProperty('--srvpro3__height', `${window.innerHeight}px`);
	document.documentElement.style.setProperty('--srvpro3__width', `${window.innerWidth}px`);
};

resize();
window.addEventListener('resize', resize);