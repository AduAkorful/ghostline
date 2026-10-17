const number = new Intl.NumberFormat("en-US");
const svgNamespace = "http://www.w3.org/2000/svg";

function svgElement(name, attributes) {
  const element = document.createElementNS(svgNamespace, name);
  for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, String(value));
  return element;
}

function cubicPoint(start, controlA, controlB, end, progress) {
  const inverse = 1 - progress;
  return {
    x: inverse ** 3 * start.x + 3 * inverse ** 2 * progress * controlA.x + 3 * inverse * progress ** 2 * controlB.x + progress ** 3 * end.x,
    y: inverse ** 3 * start.y + 3 * inverse ** 2 * progress * controlA.y + 3 * inverse * progress ** 2 * controlB.y + progress ** 3 * end.y,
  };
}

function makeHeroTicks() {
  const group = document.querySelector(".trace-ticks");
  const segments = [
    [{ x: 36, y: 33 }, { x: 180, y: 33 }, { x: 155, y: 94 }, { x: 311, y: 125 }],
    [{ x: 311, y: 125 }, { x: 390, y: 142 }, { x: 490, y: 127 }, { x: 624, y: 125 }],
  ];
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  for (let index = 0; index < 58; index += 1) {
    const segment = index < 43 ? segments[0] : segments[1];
    const span = index < 43 ? index / 42 : (index - 43) / 14;
    const point = cubicPoint(...segment, span);
    const mark = svgElement("circle", {
      cx: point.x.toFixed(2),
      cy: (point.y + Math.sin(index * 2.1) * (index < 43 ? 8 : 3)).toFixed(2),
      r: index % 7 === 0 ? 2.2 : 1.3,
    });
    const pointY = point.y + Math.sin(index * 2.1) * (index < 43 ? 8 : 3);
    mark.style.setProperty("--gather-x", `${(311 - point.x).toFixed(2)}px`);
    mark.style.setProperty("--gather-y", `${(125 - pointY).toFixed(2)}px`);
    mark.style.setProperty("--settle-x", `${(624 - point.x).toFixed(2)}px`);
    mark.style.setProperty("--settle-y", `${(125 - pointY).toFixed(2)}px`);
    mark.style.animationDelay = `${reducedMotion ? 0 : index * 28}ms`;
    group.append(mark);
  }
}

function makeVoucherMarks() {
  const group = document.querySelector(".svg-vouchers");
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  for (let index = 0; index < 24; index += 1) {
    const progress = index / 23;
    const mark = svgElement("circle", {
      cx: (110 + progress * 182).toFixed(2),
      cy: (190 + Math.sin(index * 1.7) * 5).toFixed(2),
      r: index % 5 === 0 ? 3 : 1.7,
    });
    mark.style.animationDelay = `${reducedMotion ? 0 : index * 28}ms`;
    group.append(mark);
  }
}

makeHeroTicks();
makeVoucherMarks();

const storyApp = document.querySelector("#story-app");
const storyChapters = [...document.querySelectorAll(".story-chapter")];
const storyMotionReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
let sceneStartedAt = performance.now();

function makeMotionRoute(scene, pathId, particleCount, options) {
  const path = document.getElementById(pathId);
  const layer = document.querySelector(".svg-motion-layer");
  const particles = [];
  for (let index = 0; index < particleCount; index += 1) {
    const variant = options.className;
    const halo = svgElement("circle", { class: `svg-motion-halo ${variant}`, cx: 0, cy: 0, r: 8, opacity: 0 });
    const dot = svgElement("circle", { class: `svg-motion-particle ${variant}`, cx: 0, cy: 0, r: 3.8, opacity: 0 });
    layer.append(halo, dot);
    particles.push({ halo, dot });
  }
  return { scene, path, length: path.getTotalLength(), particles, ...options };
}

const motionRoutes = [
  makeMotionRoute("service", "payment-path", 1, { className: "service-particle", duration: 1500, cadence: 2800, delay: 250, repeat: true }),
  makeMotionRoute("pay", "payment-path", 3, { className: "payment-particle", duration: 1050, cadence: 1900, stagger: 620, delay: 0, repeat: true }),
  makeMotionRoute("settle", "settlement-path", 1, { className: "settlement-particle", duration: 1350, cadence: 4200, delay: 350, repeat: true }),
  makeMotionRoute("refund", "refund-path", 1, { className: "refund-particle", duration: 1350, cadence: 4200, delay: 350, repeat: true }),
];

function hideMotionParticle(particle) {
  particle.halo.setAttribute("opacity", "0");
  particle.dot.setAttribute("opacity", "0");
}

function updateStoryMotion(now) {
  if (!storyMotionReduced && document.visibilityState === "visible") {
    const elapsed = now - sceneStartedAt;
    for (const route of motionRoutes) {
      for (let index = 0; index < route.particles.length; index += 1) {
        const particle = route.particles[index];
        if (storyApp.dataset.scene !== route.scene) {
          hideMotionParticle(particle);
          continue;
        }

        const routeTime = elapsed - (route.stagger || 0) * index - route.delay;
        if (routeTime < 0) {
          hideMotionParticle(particle);
          continue;
        }

        let progress;
        if (route.repeat) {
          const phase = routeTime % route.cadence;
          if (phase >= route.duration) {
            hideMotionParticle(particle);
            continue;
          }
          progress = phase / route.duration;
        } else {
          progress = routeTime / route.duration;
          if (progress > 1) {
            hideMotionParticle(particle);
            continue;
          }
        }

        const point = route.path.getPointAtLength(route.length * progress);
        const opacity = Math.min(1, progress * 8, (1 - progress) * 8);
        for (const element of [particle.halo, particle.dot]) {
          element.setAttribute("cx", point.x.toFixed(2));
          element.setAttribute("cy", point.y.toFixed(2));
          element.setAttribute("opacity", opacity.toFixed(2));
        }
      }
    }
  }
  if (!storyMotionReduced && document.visibilityState === "visible") requestAnimationFrame(updateStoryMotion);
}

if (!storyMotionReduced) requestAnimationFrame(updateStoryMotion);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    sceneStartedAt = performance.now();
    if (!storyMotionReduced) requestAnimationFrame(updateStoryMotion);
  }
});
const sceneLabels = {
  install: "SETUP",
  service: "SERVICE TERMS",
  prepare: "CHANNEL TERMS",
  open: "CHANNEL OPEN",
  pay: "OFF-CHAIN CALLS",
  inspect: "TWO RECORDS",
  settle: "ONE SETTLEMENT",
  refund: "EXPIRY BRANCH",
};
const sceneExplanations = {
  install: ["Start on your machine.", "Install the tools. Nothing is funded."],
  service: ["The provider sets its terms.", "It chooses the price and recipient before calls begin."],
  prepare: ["Agree on channel terms.", "Set the deposit and expiry offline, before funding."],
  open: ["Lock funds in one channel.", "One on-chain output backs many later calls."],
  pay: ["Send vouchers, not transactions.", "Each call adds a signed cumulative total off-chain."],
  inspect: ["Check both records.", "The service logs calls; the indexer confirms chain finality."],
  settle: ["Submit the latest total once.", "The service settles the cumulative voucher on-chain."],
  refund: ["Or return remaining funds.", "After expiry, the payer can refund an unspent channel output."],
};

function setScene(chapter) {
  const nextScene = chapter.dataset.scene;
  if (storyApp.dataset.scene !== nextScene) sceneStartedAt = performance.now();
  storyApp.dataset.scene = nextScene;
  chapter.classList.add("is-active");
  storyChapters.filter((other) => other !== chapter).forEach((other) => other.classList.remove("is-active"));
  const scene = chapter.dataset.scene;
  document.querySelector("#visual-scene-label").textContent = sceneLabels[scene];
  document.querySelector("#visual-explanation-title").textContent = sceneExplanations[scene][0];
  document.querySelector("#visual-explanation-copy").textContent = sceneExplanations[scene][1];
}

if ("IntersectionObserver" in window) {
  const sceneObserver = new IntersectionObserver((entries) => {
    const mostVisible = entries.filter((entry) => entry.isIntersecting).sort((a, b) => b.intersectionRatio - a.intersectionRatio)[0];
    if (mostVisible) setScene(mostVisible.target);
  }, { rootMargin: "-28% 0px -38% 0px", threshold: [0, 0.15, 0.35, 0.55] });
  storyChapters.forEach((chapter) => sceneObserver.observe(chapter));
}

const toast = document.querySelector("#toast");
let toastTimer;
function announce(message) {
  toast.textContent = message;
  toast.classList.add("visible");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("visible"), 2200);
}

async function copyText(text, successMessage, fallbackElement) {
  try {
    await navigator.clipboard.writeText(text);
    announce(successMessage);
  } catch {
    const range = document.createRange();
    range.selectNodeContents(fallbackElement);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
    announce("Select and copy the highlighted text.");
  }
}

document.querySelectorAll(".copy-command").forEach((button) => {
  button.addEventListener("click", () => {
    const code = button.closest(".command-card").querySelector("code");
    copyText(code.textContent, "Command copied. Replace placeholders before running it.", code);
  });
});

document.querySelectorAll(".copy-button").forEach((button) => {
  button.addEventListener("click", () => {
    const value = document.getElementById(button.dataset.copy);
    copyText(value.textContent, "Transaction ID copied.", value);
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

async function loadEvidence() {
  const response = await fetch("evidence.json");
  if (!response.ok) throw new Error("Evidence record could not be loaded.");
  const evidence = await response.json();
  const service = evidence.serviceRecord;
  const chain = evidence.chainRecord;
  document.querySelector("#call-count").textContent = number.format(service.successfulCalls);
  document.querySelector("#unit-price").textContent = `${number.format(service.unitPriceMicroTari)} µT`;
  document.querySelector("#accepted-total").textContent = `${number.format(service.acceptedTotalMicroTari)} µT`;
  document.querySelector("#open-tx").textContent = chain.openTransactionId;
  document.querySelector("#settle-tx").textContent = chain.settlementTransactionId;
  document.querySelector("#open-tx-link").href = chain.openTransactionUrl;
  document.querySelector("#settle-tx-link").href = chain.settlementTransactionUrl;
  document.querySelector("#refund-open-tx-link").href = evidence.refundRecord.channelOpenTransactionUrl;
  document.querySelector("#refund-tx-link").href = evidence.refundRecord.refundTransactionUrl;
  document.querySelector("#settled-total").textContent = chain.settledTotalMicroTari == null ? "Hidden" : `${number.format(chain.settledTotalMicroTari)} µT`;
  document.querySelector("#deposit-total").textContent = "Hidden";
  document.querySelector("#settlement-fee").textContent = chain.settlementFeeMicroTari == null ? "Public · see explorer" : `${number.format(chain.settlementFeeMicroTari)} µT`;
  document.querySelector("#output-status").textContent = chain.channelOutputAfterSettlement;
  document.querySelector("#evidence-date").textContent = evidence.runDate ? `Run date: ${evidence.runDate}. Static evidence; not a live network read.` : "Run date not recorded in the published evidence.";
  document.querySelectorAll(".finality").forEach((element, index) => {
    element.textContent = index === 0 ? chain.openFinality.toUpperCase() : chain.settlementFinality.toUpperCase();
  });
}

loadEvidence().catch(() => {
  document.querySelector("#evidence-grid").setAttribute("aria-label", "Evidence record unavailable");
  announce("Evidence record unavailable. Reload the page to try again.");
});
