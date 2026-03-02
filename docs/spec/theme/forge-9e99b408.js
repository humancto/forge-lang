// Forge Spec — Custom JS
// Add version badge to page title
document.addEventListener("DOMContentLoaded", function () {
  const title = document.querySelector(".content main h1");
  if (title && !title.querySelector(".version-badge")) {
    const badge = document.createElement("span");
    badge.className = "version-badge";
    badge.textContent = "v0.3.3";
    badge.style.marginLeft = "0.8rem";
    badge.style.verticalAlign = "middle";
    badge.style.fontSize = "0.5em";
    title.appendChild(badge);
  }
});
