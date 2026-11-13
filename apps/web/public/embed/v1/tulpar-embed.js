(function () {
  "use strict";

  var script = document.currentScript;
  var defaultInstance = script
    ? new URL(script.src).origin
    : window.location.origin;

  function normalizeInstance(value) {
    return String(value || defaultInstance).replace(/\/+$/, "");
  }

  function mount(element, options) {
    var key = options && options.key;
    if (!key) {
      throw new Error("TulparEmbed.mount requires a public key.");
    }
    var widget = options.widget || "community";
    var instance = normalizeInstance(options.instance);
    var origin = window.location.origin;
    var iframe = document.createElement("iframe");
    iframe.title = options.title || "Tulpar embed";
    iframe.loading = "lazy";
    iframe.src =
      instance +
      "/embed/widget?key=" +
      encodeURIComponent(key) +
      "&widget=" +
      encodeURIComponent(widget) +
      "&origin=" +
      encodeURIComponent(origin);
    iframe.style.border = "0";
    iframe.style.width = "100%";
    iframe.style.height = (options.height || 240) + "px";
    iframe.style.display = "block";
    iframe.setAttribute("allowtransparency", "true");
    element.replaceChildren(iframe);

    function onMessage(event) {
      if (event.origin !== instance) {
        return;
      }
      var data = event.data;
      if (!data || data.type !== "tulpar:resize") {
        return;
      }
      if (data.widget && data.widget !== widget) {
        return;
      }
      if (typeof data.height === "number") {
        iframe.style.height =
          Math.max(120, Math.min(800, Math.round(data.height))) + "px";
      }
    }

    window.addEventListener("message", onMessage);
    return {
      iframe: iframe,
      destroy: function () {
        window.removeEventListener("message", onMessage);
        iframe.remove();
      },
    };
  }

  function autoMount() {
    var elements = document.querySelectorAll("[data-tulpar-embed]");
    for (var index = 0; index < elements.length; index += 1) {
      var element = elements[index];
      var key = element.getAttribute("data-key");
      if (!key) {
        continue;
      }
      mount(element, {
        key: key,
        widget: element.getAttribute("data-widget") || "community",
        instance: element.getAttribute("data-instance") || undefined,
        height: Number(element.getAttribute("data-height")) || undefined,
        title: element.getAttribute("data-title") || undefined,
      });
    }
  }

  window.TulparEmbed = { mount: mount };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", autoMount);
  } else {
    autoMount();
  }
})();
