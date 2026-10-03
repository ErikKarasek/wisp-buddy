// One page, two windows: the buddy on the desktop, and the studio where its look is made.
const view = new URLSearchParams(location.search).get("view");

if (view === "studio") (await import("./studio")).startStudio();
else (await import("./buddy")).startBuddy();

export {};
