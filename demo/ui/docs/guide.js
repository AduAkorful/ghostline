const copyStatus = document.querySelector("#copy-status");
let copyStatusTimer;

function showCopyStatus(message) {
  copyStatus.textContent = message;
  copyStatus.classList.add("visible");
  clearTimeout(copyStatusTimer);
  copyStatusTimer = setTimeout(() => copyStatus.classList.remove("visible"), 2400);
}

document.querySelectorAll(".guide-copy").forEach((button) => {
  button.addEventListener("click", async () => {
    const content = document.getElementById(button.dataset.copy);
    const text = content.textContent;
    try {
      await navigator.clipboard.writeText(text);
      showCopyStatus("Copied to clipboard.");
    } catch {
      const range = document.createRange();
      range.selectNodeContents(content);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      showCopyStatus("Command selected. Copy it from the selection.");
    }
  });
});

document.querySelectorAll(".mobile-nav").forEach((navigation) => {
  navigation.querySelectorAll("a").forEach((link) => {
    link.addEventListener("click", () => { navigation.open = false; });
  });
  navigation.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && navigation.open) {
      navigation.open = false;
      navigation.querySelector("summary").focus();
    }
  });
});
