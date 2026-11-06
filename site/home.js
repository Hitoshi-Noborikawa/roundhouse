// Home page behaviour.
(function () {
  // The primary buttons' shimmer pauses while they are off-screen.
  if ("IntersectionObserver" in window) {
    var watch = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) { entry.target.classList.toggle("is-offscreen", !entry.isIntersecting); });
    });
    document.querySelectorAll(".button.primary").forEach(function (button) { watch.observe(button); });
  }

  // One-click copy for the install commands. The buttons stay hidden without the Clipboard API.
  if (!navigator.clipboard) return;
  document.querySelectorAll(".copy-btn[data-copy]").forEach(function (button) {
    var label = button.querySelector("span");
    var icon = button.querySelector("use");
    var timer;
    button.hidden = false;
    function show(state, text, iconId) {
      button.classList.remove("is-copied", "is-failed");
      if (state) button.classList.add(state);
      label.textContent = text;
      icon.setAttribute("href", iconId);
    }
    button.addEventListener("click", function () {
      clearTimeout(timer);
      navigator.clipboard.writeText(button.dataset.copy).then(function () {
        show("is-copied", "Copied", "#icon-check");
      }, function () {
        // The browser refused the write: select the command so a keyboard copy works.
        var range = document.createRange();
        range.selectNodeContents(button.closest(".cmd").querySelector(".cmd-text"));
        var selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        show("is-failed", "Copy failed; the command is selected", "#icon-copy");
      }).then(function () {
        timer = setTimeout(function () { show(null, "Copy", "#icon-copy"); }, 1600);
      });
    });
  });
})();
