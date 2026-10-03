// One page, three windows: the buddy on the desktop, the studio where its look is made, and
// the chat bubble above its head.
const view = new URLSearchParams(location.search).get("view");

if (view === "studio") await (await import("./studio")).startStudio();
else if (view === "chat") await (await import("./chat")).startChat();
else await (await import("./buddy")).startBuddy();

export {};
