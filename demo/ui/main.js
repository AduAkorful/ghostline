const number = new Intl.NumberFormat("en-US");

function microTari(value) {
  return `${number.format(value)} µT`;
}

async function loadEvidence() {
  const response = await fetch("evidence.json");
  if (!response.ok) throw new Error("Evidence record could not be loaded.");
  const evidence = await response.json();
  const service = evidence.serviceRecord;
  const chain = evidence.chainRecord;

  document.querySelector("#call-count").textContent = number.format(service.successfulCalls);
  document.querySelector("#unit-price").textContent = microTari(service.unitPriceMicroTari);
  document.querySelector("#accepted-total").textContent = microTari(service.acceptedTotalMicroTari);
  document.querySelector("#open-tx").textContent = chain.openTransactionId;
  document.querySelector("#settle-tx").textContent = chain.settlementTransactionId;
  document.querySelector("#settled-total").textContent = microTari(chain.settledTotalMicroTari);
  document.querySelector("#settlement-fee").textContent = microTari(chain.settlementFeeMicroTari);
  document.querySelector("#output-status").textContent = chain.channelOutputAfterSettlement;
  document.querySelector("#evidence-date").textContent = evidence.runDate ? `Run date: ${evidence.runDate}` : "Historical run; run date not recorded in the published evidence.";
  document.querySelectorAll(".finality").forEach((el, index) => {
    el.textContent = index === 0 ? chain.openFinality.toUpperCase() : chain.settlementFinality.toUpperCase();
  });
}

const toast = document.querySelector("#toast");
let toastTimer;

function announce(message) {
  toast.textContent = message;
  toast.classList.add("visible");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("visible"), 2200);
}

document.querySelectorAll(".copy-button").forEach((button) => {
  button.addEventListener("click", async () => {
    const text = document.getElementById(button.dataset.copy).textContent;
    try {
      await navigator.clipboard.writeText(text);
      announce("Transaction ID copied");
    } catch {
      const range = document.createRange();
      range.selectNodeContents(document.getElementById(button.dataset.copy));
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      announce("Select and copy the highlighted transaction ID");
    }
  });
});

loadEvidence().catch(() => {
  document.querySelector("#evidence-grid").setAttribute("aria-label", "Evidence record unavailable");
  announce("Evidence record unavailable. Reload the page to try again.");
});
