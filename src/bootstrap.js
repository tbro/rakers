// rakers browser-globals bootstrap.
// Evaluated before every user script. __HREF__ is replaced by Rust with the page URL.

var window = globalThis;
var self   = window;
var global = window;

// Node.js-style globals that bundlers (webpack/vite) reference at runtime.
// React/Vue/Angular all gate dev-mode warnings on process.env.NODE_ENV.
var process = {
    env: { NODE_ENV: 'production' },
    browser: true, version: 'v18.0.0', versions: {},
    nextTick: function(fn) {}  // some polyfill shims (e.g. promise-polyfill) call process.nextTick
};

// ─── URL parser (used by element setAttribute and window.URL) ───────────────

function _r_parse_url(href, base) {
    var s = String(href || '');
    // Resolve relative URL against base
    if (base && !/^[a-zA-Z][a-zA-Z0-9+\-.]*:/.test(s)) {
        var b = _r_parse_url(base);
        if (s.charAt(0) === '/') {
            s = b.protocol + '//' + b.host + s;
        } else if (s.charAt(0) === '#') {
            s = b.protocol + '//' + b.host + b.pathname + s;
        } else {
            var dir = b.pathname.replace(/\/[^\/]*$/, '/');
            s = b.protocol + '//' + b.host + dir + s;
        }
    }
    var m = s.match(/^([a-zA-Z][a-zA-Z0-9+\-.]*:)\/\/([^\/\?#]*)([^\?#]*)(\?[^#]*)?(#.*)?$/);
    if (m) {
        var protocol = m[1] || '';
        var host     = m[2] || '';
        var hostname = host.replace(/:\d+$/, '');
        var port     = (host.match(/:(\d+)$/) || ['', ''])[1];
        var pathname = m[3] || '/';
        var search   = m[4] || '';
        var hash     = m[5] || '';
        return { href: s, protocol: protocol, host: host, hostname: hostname,
                 port: port, pathname: pathname, search: search, hash: hash,
                 origin: protocol + '//' + host };
    }
    return { href: s, protocol: '', host: '', hostname: '', port: '',
             pathname: s || '/', search: '', hash: '', origin: '' };
}

// ─── Element factory ────────────────────────────────────────────────────────

function _r_el(tag) {
    tag = (tag || 'DIV').toUpperCase();
    var el = {
        tagName: tag, nodeName: tag, nodeType: 1,
        // Elm VirtualDom: _VirtualDom_virtualize iterates node.attributes to rebuild vdom from existing DOM
        attributes: [],
        id: '', className: '', name: '', type: '', value: '',
        href: '', src: '', alt: '', placeholder: '',
        // URL-derived properties (populated when href is set on anchor/link elements)
        protocol: '', host: '', hostname: '', port: '', pathname: '',
        search: '', hash: '', origin: '',
        // React: event delegation walks node.ownerDocument to find the root container
        get ownerDocument() { return typeof document !== 'undefined' ? document : null; },
        style: {}, dataset: {},
        // _ihtml: base HTML set directly via innerHTML setter
        // _kids:  child nodes appended via appendChild (stored as live references)
        // innerHTML is defined below as a getter/setter via Object.defineProperty
        _ihtml: '', _kids: [],
        parentNode: null, parentElement: null,
        // Angular/Vue: classList.add/remove/contains used for dynamic class binding
        classList: {
            _c: [],
            add:      function(c) { if (this._c.indexOf(c) < 0) this._c.push(c); },
            remove:   function(c) { this._c = this._c.filter(function(x) { return x !== c; }); },
            toggle:   function(c) { if (this._c.indexOf(c) >= 0) this.remove(c); else this.add(c); },
            contains: function(c) { return this._c.indexOf(c) >= 0; },
            toString: function()  { return this._c.join(' '); },
            length: 0
        },
        addEventListener: function() {}, removeEventListener: function() {},
        dispatchEvent: function() { return true; },
        setAttribute: function(n, v) {
            v = String(v);
            if      (n === 'id')    this.id    = v;
            else if (n === 'class') this.className = v;
            else if (n === 'href') {
                this.href = v;
                var u = _r_parse_url(v, window.location && window.location.href);
                this.protocol = u.protocol; this.host     = u.host;
                this.hostname = u.hostname; this.port     = u.port;
                this.pathname = u.pathname; this.search   = u.search;
                this.hash     = u.hash;     this.origin   = u.origin;
            }
            else if (n === 'src')   this.src   = v;
            else if (n === 'type')  this.type  = v;
            else if (n === 'value') this.value = v;
            else if (n === 'name')  this.name  = v;
        },
        getAttribute: function(n) {
            if (n === 'id')       return this.id        || null;
            if (n === 'class')    return this.className || null;
            if (n === 'href')     return this.href      || null;
            if (n === 'src')      return this.src       || null;
            if (n === 'pathname') return this.pathname  || null;
            if (n === 'hostname') return this.hostname  || null;
            return null;
        },
        hasAttribute:    function(n) { return !!this.getAttribute(n); },
        removeAttribute: function() {},
        // All VirtualDom frameworks (React, Vue, Mithril, Elm): appendChild/insertBefore/removeChild
        // are the primary DOM-building primitives. insertBefore and removeChild must mutate _kids
        // so that childNodes[i] indexing (used by Mithril/Elm diffing) stays correct.
        appendChild: function(child) {
            if (child) {
                this._kids.push(child);
                if (typeof child === 'object') {
                    child.parentNode    = this;
                    child.parentElement = this;
                }
            }
            return child;
        },
        prepend: function(child) {
            if (child && typeof child === 'object') {
                this._kids.unshift(child);
                child.parentNode    = this;
                child.parentElement = this;
            } else if (typeof child === 'string') {
                this._ihtml = child + this._ihtml;
            }
        },
        append: function(child) {
            if (typeof child === 'string') this._ihtml += child;
            else this.appendChild(child);
        },
        insertBefore: function(n, ref) {
            if (!n) return n;
            if (ref == null) return this.appendChild(n);
            var idx = this._kids.indexOf(ref);
            if (idx >= 0) this._kids.splice(idx, 0, n);
            else this._kids.push(n);
            if (typeof n === 'object') { n.parentNode = this; n.parentElement = this; }
            return n;
        },
        removeChild: function(c) {
            var idx = this._kids.indexOf(c);
            if (idx >= 0) this._kids.splice(idx, 1);
            return c;
        },
        replaceChild: function(n, o) {
            var idx = this._kids.indexOf(o);
            if (idx >= 0) { this._kids[idx] = n; if (typeof n === 'object') { n.parentNode = this; n.parentElement = this; } }
            else this._kids.push(n);
            return o;
        },
        cloneNode:       function(deep) { var c = _r_el(this.tagName); if (deep) c.innerHTML = this.innerHTML; return c; },
        contains:        function()     { return false; },
        closest:         function()     { return null; },
        matches:         function()     { return false; },
        querySelector:   function()     { return null; },
        querySelectorAll:function()     { return []; },
        getBoundingClientRect: function() { return {top:0,left:0,bottom:0,right:0,width:0,height:0,x:0,y:0}; },
        getClientRects:        function() { return []; },
        focus: function() {}, blur: function() {}, click: function() {},
        scrollIntoView: function() {}, scrollTo: function() {}, scroll: function() {},
        insertAdjacentHTML:    function(pos, html) { this._ihtml += html; },
        insertAdjacentElement: function(pos, el)   { return this.appendChild(el); },
        insertAdjacentText:    function(pos, text) { this._ihtml += _r_esc(text); },
        hasChildNodes: function() { return this._kids.length > 0 || this._ihtml.length > 0; },
        normalize: function() {},
        before: function() {}, after: function() {}, remove: function() {}, replaceWith: function() {},
        requestPointerLock: function() {},
        animate: function() { return { finished: Promise.resolve(), cancel: function(){} }; }
    };
    // innerHTML getter: lazily serializes _kids so parent.appendChild(child) followed
    // by child.appendChild(grandchild) produces the correct tree at readback time.
    // Setter clears _kids and replaces the base HTML string.
    Object.defineProperty(el, 'innerHTML', {
        get: function() {
            if (el._kids.length === 0) return el._ihtml;
            var s = el._ihtml;
            for (var i = 0; i < el._kids.length; i++) {
                var c = el._kids[i];
                if (c.nodeType === 3) s += _r_esc(c.nodeValue || '');
                else if (c.tagName)   s += _r_serialize(c);
                else if (typeof c === 'string') s += c;
            }
            return s;
        },
        set: function(v) { el._kids = []; el._ihtml = (v == null ? '' : String(v)); },
        configurable: true,
        enumerable: true
    });
    Object.defineProperty(el, 'textContent', {
        get: function() {
            return el.innerHTML.replace(/<[^>]*>/g, '');
        },
        set: function(v) {
            el.innerHTML = _r_esc(v == null ? '' : String(v));
        },
        configurable: true
    });
    // Mithril, Elm: VirtualDom patching indexes childNodes[i] to find insertion/removal
    // points. Must be a live view of _kids, not a static empty array.
    Object.defineProperty(el, 'childNodes', {
        get: function() { return el._kids; },
        configurable: true
    });
    Object.defineProperty(el, 'children', {
        get: function() { return el._kids.filter(function(k) { return k && (k.nodeType === 1 || k.tagName); }); },
        configurable: true
    });
    if (tag === 'TEMPLATE') {
        Object.defineProperty(el, 'content', {
            get: function() { var f = _r_el('div'); f.innerHTML = el.innerHTML; return f; },
            configurable: true
        });
    }
    return el;
}

// ─── Serializer ──────────────────────────────────────────────────────────────

var _r_void = 'area,base,br,col,embed,hr,img,input,link,meta,param,source,track,wbr'.split(',');

function _r_serialize(el) {
    if (!el) return '';
    if (el.nodeType === 3 || (typeof el.nodeValue === 'string' && !el.tagName))
        return _r_esc(el.nodeValue || el.textContent || '');
    if (typeof el.tagName !== 'string') return _r_esc(String(el.textContent || ''));
    var tag = el.tagName.toLowerCase();
    var a = '';
    if (el.id)          a += ' id="'          + _r_esc_a(el.id) + '"';
    var cls = el.className ||
              (el.classList && typeof el.classList.toString === 'function' ? el.classList.toString() : '');
    if (cls)            a += ' class="'        + _r_esc_a(cls) + '"';
    if (el.href)        a += ' href="'         + _r_esc_a(el.href) + '"';
    if (el.src)         a += ' src="'          + _r_esc_a(el.src) + '"';
    if (el.type)        a += ' type="'         + _r_esc_a(el.type) + '"';
    if (el.value)       a += ' value="'        + _r_esc_a(el.value) + '"';
    if (el.name)        a += ' name="'         + _r_esc_a(el.name) + '"';
    if (el.alt)         a += ' alt="'          + _r_esc_a(el.alt) + '"';
    if (el.placeholder) a += ' placeholder="'  + _r_esc_a(el.placeholder) + '"';
    if (_r_void.indexOf(tag) >= 0) return '<' + tag + a + '>';
    return '<' + tag + a + '>' + (el.innerHTML || '') + '</' + tag + '>';
}

function _r_esc(s)   { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
function _r_esc_a(s) { return String(s).replace(/&/g,'&amp;').replace(/"/g,'&quot;'); }

// ─── Element registry ────────────────────────────────────────────────────────

var _r_reg = {};

// ─── document ────────────────────────────────────────────────────────────────

document.createElement    = _r_el;
document.createElementNS  = function(ns, tag) { return _r_el(tag); };  // Vue/Angular: SVG elements created via createElementNS
document.createTextNode   = function(t) { return {nodeType:3, nodeValue:String(t), textContent:String(t)}; };
document.createComment    = function(t) { return {nodeType:8, nodeValue:t}; };
document.createDocumentFragment = function() { return _r_el('div'); };  // React/Angular: mount root appended to a fragment first
document.createRange      = function() {
    return {
        selectNodeContents: function() {},
        toString: function() { return ''; },
        createContextualFragment: function(html) { var d=_r_el('div'); d.innerHTML=html; return d; }
    };
};
document.createEvent = function() { return {initEvent:function(){}, type:'', bubbles:false, cancelable:false}; };
document.nodeType = 9; // React: checks node.nodeType === 9 to identify the document root when wiring synthetic events

document.getElementById = function(id) {
    if (!_r_reg[id]) { var e = _r_el('div'); e.id = id; _r_reg[id] = e; }
    return _r_reg[id];
};
document.getElementsByClassName = function() { return []; };
document.getElementsByTagName   = function(tag) {
    if (!tag) return [];
    var t = tag.toUpperCase();
    if (t === 'HEAD')   return [document.head];
    if (t === 'BODY')   return [document.body];
    if (t === 'HTML')   return [document.documentElement];
    // webpack/GA: `getElementsByTagName('script')[0].parentNode` to find a DOM insertion point
    if (t === 'SCRIPT') return [document.currentScript];
    return [];
};
document.getElementsByName      = function() { return []; };
document.querySelector = function(sel) {
    if (!sel) return null;
    // #id selector
    var m = sel.match(/^#([\w-]+)$/);
    if (m) return document.getElementById(m[1]);
    // bare tag selectors (used by style-loader, css-in-js, analytics snippets)
    var tag = sel.replace(/[:\s>+~[*].*$/, '').trim().toUpperCase();
    if (tag === 'HEAD')   return document.head;
    if (tag === 'BODY')   return document.body;
    if (tag === 'HTML')   return document.documentElement;
    if (tag === 'SCRIPT') return document.currentScript;
    // Mithril: mounts via querySelector('.todoapp'); Angular: querySelector('app-root').
    // Return document.body so the framework mounts into our captured DOM rather than null.
    if (/^\.[a-zA-Z][\w-]*$/.test(sel)) return document.body;  // .class selector
    if (/^[a-z][a-z0-9]*(-[a-z0-9]+)+$/.test(sel)) return document.body;  // custom-element name
    return null;
};
document.querySelectorAll = function(sel) {
    var el = document.querySelector(sel);
    return el ? [el] : [];
};

document.body            = _r_el('body');
document.head            = _r_el('head');
document.documentElement = _r_el('html');
document.readyState      = 'complete';
document.cookie          = '';
document.referrer        = '';
document.domain          = '';
document.title           = '';
document.addEventListener    = function() {};
document.removeEventListener = function() {};
document.dispatchEvent       = function() {};
document.execCommand         = function() { return false; };
document.hasFocus            = function() { return false; };
document.getSelection        = function() { return null; };
document.elementFromPoint    = function() { return null; };
document.elementsFromPoint   = function() { return []; };
document.activeElement       = null;
document.defaultView         = window;
// webpack: reads document.currentScript.src to determine the public asset path.
// SvelteKit: reads document.currentScript.parentElement to detect the mount context.
document.currentScript = {
    src: '__HREF__', type: 'text/javascript', nodeType: 1, tagName: 'SCRIPT',
    parentElement: document.head, parentNode: document.head,
    getAttribute: function(n) { return n === 'src' ? this.src : n === 'type' ? this.type : null; },
    setAttribute: function() {}, hasAttribute: function(n) { return n === 'src' || n === 'type'; }
};

// ─── window ──────────────────────────────────────────────────────────────────

(function() {
    var _u = _r_parse_url("__HREF__");
    var _hash = _u.hash;
    var _loc = {
        href:     _u.href,     protocol: _u.protocol, host:     _u.host,
        hostname: _u.hostname, port:     _u.port,     pathname: _u.pathname,
        search:   _u.search,   origin:   _u.origin,
        assign: function() {}, replace: function() {}, reload: function() {},
        toString: function() { return this.href; }
    };
    // Mithril: sets window.location.hash = '#/' when no route matches, then relies on
    // window.onhashchange to re-trigger routing and render the default route.
    Object.defineProperty(_loc, 'hash', {
        get: function() { return _hash; },
        set: function(v) {
            var prev = _hash;
            _hash = String(v);
            if (prev !== _hash) {
                _r_timers.push(function() {
                    if (typeof window.onhashchange === 'function') {
                        try { window.onhashchange({ type: 'hashchange', oldURL: prev, newURL: _hash }); } catch(e) {}
                    }
                });
            }
        },
        enumerable: true, configurable: true
    });
    window.location = _loc;
    document.location = _loc;
})();
window.navigator = {
    userAgent: 'rakers/0.1.0', appName: 'rakers', appVersion: '0.1.0',
    language: 'en-US', languages: ['en-US', 'en'],
    platform: 'Linux', vendor: '',
    onLine: false, cookieEnabled: false,
    javaEnabled: function() { return false; }
};
window.screen = {width:1920, height:1080, availWidth:1920, availHeight:1080, colorDepth:24};
// Angular router uses history.pushState for navigation; Mithril uses replaceState
// during its initial route redirect (m.route with mode='hash').
window.history = {
    length: 1, scrollRestoration: 'auto', state: null,
    pushState:    function(s) { this.state = s || null; },
    replaceState: function(s) { this.state = s || null; },
    back: function() {}, forward: function() {}, go: function() {}
};
window.performance = {
    now:               function() { return 0; },
    mark:              function() {}, measure: function() {},
    getEntriesByType:  function() { return []; },
    getEntriesByName:  function() { return []; },
    clearMarks:        function() {}, clearMeasures: function() {},
    timing:  { navigationStart: 0, domContentLoadedEventEnd: 0, loadEventEnd: 0 },
    memory:  { usedJSHeapSize: 0, jsHeapSizeLimit: 2147483648 }
};
// Mithril TodoMVC: persists todos to localStorage; Aurelia and Backbone TodoMVC do the same.
window.localStorage = {
    _s: {}, length: 0,
    getItem:    function(k) { return Object.prototype.hasOwnProperty.call(this._s, k) ? this._s[k] : null; },
    setItem:    function(k, v) { this._s[k] = String(v); },
    removeItem: function(k) { delete this._s[k]; },
    clear:      function()  { this._s = {}; },
    key:        function()  { return null; }
};
window.sessionStorage = {
    _s: {}, length: 0,
    getItem:    function(k) { return Object.prototype.hasOwnProperty.call(this._s, k) ? this._s[k] : null; },
    setItem:    function(k, v) { this._s[k] = String(v); },
    removeItem: function(k) { delete this._s[k]; },
    clear:      function()  { this._s = {}; },
    key:        function()  { return null; }
};
// Deferred callbacks flushed in a loop after all scripts run (see READBACK_JS in runtime.rs).
// setTimeout: Backbone/KnockoutJS defer their initial render via setTimeout(fn, 0).
// requestAnimationFrame: Mithril schedules redraws via rAF; Vue 2 also uses rAF as a nextTick fallback.
// queueMicrotask: Vue 3 nextTick uses queueMicrotask when available.
var _r_timers = [];
window.setTimeout            = function(fn, delay) { if (typeof fn === 'function') _r_timers.push(fn); return _r_timers.length; };
window.clearTimeout          = function(id) {};
window.setInterval           = function(fn, delay) { return 0; };
window.clearInterval         = function(id) {};
window.requestAnimationFrame = function(fn) { if (typeof fn === 'function') _r_timers.push(fn); return _r_timers.length; };
window.cancelAnimationFrame  = function(id) {};
window.queueMicrotask        = function(fn) { if (typeof fn === 'function') _r_timers.push(fn); };
window.alert   = function(msg) {};
window.confirm = function(msg) { return false; };
window.prompt  = function(msg, def) { return null; };
window.open    = function() { return null; };
window.close   = function() {};
window.postMessage     = function() {};
window.fetch = function(url) {
    var res = {
        ok: true, status: 200, statusText: 'OK',
        url: String(url || ''), redirected: false, type: 'basic',
        headers: { get: function() { return null; }, has: function() { return false; },
                   forEach: function() {}, entries: function() { return []; } },
        json:        function() { return Promise.resolve(null); },
        text:        function() { return Promise.resolve(''); },
        blob:        function() { return Promise.resolve(new window.Blob()); },
        arrayBuffer: function() { return Promise.resolve(null); },
        clone:       function() { return this; }
    };
    return Promise.resolve(res);
};
// RiotJS: loads component templates at runtime via XHR (src="todo.html" in a riot/tag script).
// The send() stub queues a timer so onload fires after scripts finish; responseText is empty
// which is why RiotJS doesn't render (would need a real HTTP GET to fix).
window.XMLHttpRequest  = function() {
    var self = this;
    this.readyState=0; this.status=0; this.statusText='';
    this.responseText=''; this.responseXML=null; this.response='';
    this.responseType=''; this.withCredentials=false; this.timeout=0;
    this.onreadystatechange=null; this.onload=null; this.onerror=null;
    this.onprogress=null; this.ontimeout=null; this.onabort=null;
    this.open=function(){};
    this.send=function() {
        _r_timers.push(function() {
            self.readyState=4; self.status=200; self.statusText='OK';
            if (typeof self.onreadystatechange==='function') try { self.onreadystatechange.call(self); } catch(e) {}
            if (typeof self.onload==='function')             try { self.onload.call(self, {target:self}); } catch(e) {}
        });
    };
    this.abort=function(){};
    this.setRequestHeader=function(){};
    this.getResponseHeader=function(){return null;};
    this.getAllResponseHeaders=function(){return '';};
    this.overrideMimeType=function(){};
    this.addEventListener=function(t,fn){
        if (t==='load') self.onload=fn;
        else if (t==='error') self.onerror=fn;
        else if (t==='readystatechange') self.onreadystatechange=fn;
    };
    this.removeEventListener=function(){};
};
window.FormData = function() {
    this.append=function(){}; this.delete=function(){};
    this.get=function(){return null;}; this.has=function(){return false;};
    this.set=function(){};
};
window.URL = function(href, base) {
    var u = _r_parse_url(String(href), base ? String(base) : (window.location && window.location.href));
    this.href = u.href; this.protocol = u.protocol; this.host = u.host;
    this.hostname = u.hostname; this.port = u.port; this.pathname = u.pathname;
    this.search = u.search; this.hash = u.hash; this.origin = u.origin;
    this.toString = function() { return this.href; };
    this.searchParams = new window.URLSearchParams(u.search);
};
window.URL.createObjectURL = function() { return ''; };
window.URL.revokeObjectURL = function() {};
window.Blob       = function(parts, opts) { this.size=0; this.type=(opts&&opts.type)||''; };
window.FileReader = function() { this.readAsText=function(){}; this.readAsDataURL=function(){}; this.addEventListener=function(){}; };
window.matchMedia   = function(q) {
    return {matches:false, media:q, addEventListener:function(){}, removeEventListener:function(){}, addListener:function(){}, removeListener:function(){}};
};
window.getComputedStyle = function(el) { return {}; };
window.requestIdleCallback  = function(fn) { return 0; };
window.cancelIdleCallback   = function(id) {};
window.MutationObserver     = function(cb) { this.observe=function(){}; this.disconnect=function(){}; this.takeRecords=function(){return [];}; };  // Angular zone.js, Vue: patch MutationObserver to detect async DOM changes
window.ResizeObserver       = function(cb) { this.observe=function(){}; this.disconnect=function(){}; this.unobserve=function(){}; };
window.IntersectionObserver = function(cb) { this.observe=function(){}; this.disconnect=function(){}; this.unobserve=function(){}; };
window.PerformanceObserver  = function(cb) { this.observe=function(){}; this.disconnect=function(){}; };
window.CustomEvent  = function(type, init) { this.type=type; this.detail=init&&init.detail||null; this.bubbles=false; this.cancelable=false; };  // web-components, Angular: dispatch custom events
window.Event        = function(type, init) { this.type=type; this.bubbles=!!(init&&init.bubbles); this.cancelable=!!(init&&init.cancelable); };
window.KeyboardEvent= window.Event;
window.MouseEvent   = window.Event;
window.TouchEvent   = window.Event;
window.InputEvent   = window.Event;
window.FocusEvent   = window.Event;
window.ErrorEvent   = window.Event;
window.MessageEvent = function(type, init) { this.type=type; this.data=init&&init.data||null; };
window.PointerEvent = window.Event;
window.WheelEvent   = window.Event;
// React scheduler (>=17): uses MessageChannel to schedule work as a macrotask,
// ensuring renders happen after browser paint. port1.postMessage triggers port2.onmessage.
window.MessageChannel = function() {
    var self = this;
    this.port1 = { onmessage: null, postMessage: function(msg) {
        if (typeof self.port2.onmessage === 'function') _r_timers.push(function(){ self.port2.onmessage({data:msg}); });
    }};
    this.port2 = { onmessage: null, postMessage: function(msg) {
        if (typeof self.port1.onmessage === 'function') _r_timers.push(function(){ self.port1.onmessage({data:msg}); });
    }};
};
window.addEventListener    = function() {};
window.removeEventListener = function() {};
window.dispatchEvent       = function() { return true; };
// Google Analytics / GTM: many pages include GA4 which references dataLayer and gtag
window.dataLayer = [];
window.gtag = function() { window.dataLayer.push(arguments); };
window.btoa = function(str) {
    var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
    str = String(str); var out = '';
    for (var i = 0; i < str.length; i += 3) {
        var b0=str.charCodeAt(i), b1=i+1<str.length?str.charCodeAt(i+1):0, b2=i+2<str.length?str.charCodeAt(i+2):0;
        out += chars[b0>>2] + chars[((b0&3)<<4)|(b1>>4)];
        out += i+1<str.length ? chars[((b1&15)<<2)|(b2>>6)] : '=';
        out += i+2<str.length ? chars[b2&63] : '=';
    }
    return out;
};
window.atob = function(str) {
    var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
    str = String(str).replace(/[\s=]+$/g, ''); var out = '';
    for (var i = 0; i < str.length; i += 4) {
        var c0=chars.indexOf(str[i]), c1=chars.indexOf(str[i+1]);
        var c2=i+2<str.length?chars.indexOf(str[i+2]):64, c3=i+3<str.length?chars.indexOf(str[i+3]):64;
        out += String.fromCharCode((c0<<2)|(c1>>4));
        if (c2!==64) out += String.fromCharCode(((c1&15)<<4)|(c2>>2));
        if (c3!==64) out += String.fromCharCode(((c2&3)<<6)|c3);
    }
    return out;
};
window.AbortController = function() { this.signal={aborted:false,addEventListener:function(){}}; this.abort=function(){}; };
window.AbortSignal  = {timeout:function(){return {aborted:false,addEventListener:function(){}};}};
window.TextEncoder  = function() { this.encode=function(s){return new Uint8Array(0);}; };
window.TextDecoder  = function() { this.decode=function(b){return '';}; };
window.crypto       = {getRandomValues:function(a){return a;}, subtle:{}, randomUUID:function(){return '00000000-0000-0000-0000-000000000000';}};
window.CSS          = {supports:function(){return false;}, escape:function(s){return s;}};
window.DOMException = function(msg, name) { this.message=msg||''; this.name=name||'Error'; this.code=0; };
window.DOMException.prototype = Object.create(Error.prototype);
// web-components framework: calls customElements.define() to register <todo-app> etc.;
// whenDefined() is awaited before mounting.
window.customElements = {
    define: function() {}, get: function() { return undefined; },
    upgrade: function() {}, whenDefined: function() { return Promise.resolve(); }
};
window.URLSearchParams = function(init) {
    this._p = {};
    var s = typeof init === 'string' ? init.replace(/^\?/, '') : '';
    if (s) s.split('&').forEach(function(pair) {
        var i = pair.indexOf('=');
        var k = decodeURIComponent(i < 0 ? pair : pair.slice(0, i));
        var v = i < 0 ? '' : decodeURIComponent(pair.slice(i + 1));
        if (k) this._p[k] = v;
    }, this);
    this.get    = function(k) { return Object.prototype.hasOwnProperty.call(this._p, k) ? this._p[k] : null; };
    this.has    = function(k) { return Object.prototype.hasOwnProperty.call(this._p, k); };
    this.set    = function(k, v) { this._p[k] = String(v); };
    this.delete = function(k) { delete this._p[k]; };
    this.toString = function() {
        return Object.keys(this._p).map(function(k) {
            return encodeURIComponent(k) + '=' + encodeURIComponent(this._p[k]);
        }, this).join('&');
    };
};
// web-components: custom elements extend HTMLElement; the constructor check
// `el instanceof HTMLElement` must not throw.
window.HTMLElement         = function() {};
window.HTMLTemplateElement = function() {};
window.HTMLIFrameElement   = function() {};
window.HTMLInputElement    = function() {};
window.HTMLTextAreaElement = function() {};
window.HTMLSelectElement   = function() {};
window.HTMLButtonElement   = function() {};
window.HTMLAnchorElement   = function() {};
window.HTMLImageElement    = function() {};
window.HTMLFormElement     = function() {};
window.HTMLScriptElement   = function() {};
window.HTMLLinkElement     = function() {};
window.HTMLDivElement      = function() {};
window.HTMLSpanElement     = function() {};
window.Element      = function() {};
window.Node         = function() {};
window.EventTarget  = function() {};
window.SVGElement   = function() {};  // Vue: checks `el instanceof SVGElement` when deciding how to create elements
window.Document     = function() {};
window.DocumentFragment = function() {};
window.Window       = function() {};
window.ShadowRoot   = function() {};  // web-components, Svelte: shadow DOM host check
window.devicePixelRatio = 1;
window.innerWidth  = 1920; window.innerHeight = 1080;
window.outerWidth  = 1920; window.outerHeight = 1080;
window.pageXOffset = 0;    window.pageYOffset = 0;
window.scrollX     = 0;    window.scrollY     = 0;
window.scrollTo    = function() {}; window.scroll   = function() {}; window.scrollBy = function() {};
window.print       = function() {}; window.focus    = function() {}; window.blur     = function() {};
