// rakers browser-globals bootstrap.
// Evaluated before every user script. __HREF__ is replaced by Rust with the page URL.

var window = globalThis;
var self   = window;
var global = window;

// Node.js-style globals that bundlers (webpack/vite) reference at runtime.
var process = {
    env: { NODE_ENV: 'production' },
    browser: true, version: 'v18.0.0', versions: {},
    nextTick: function(fn) {}
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
        tagName: tag, nodeType: 1,
        id: '', className: '', name: '', type: '', value: '',
        href: '', src: '', alt: '', placeholder: '',
        // URL-derived properties (populated when href is set on anchor/link elements)
        protocol: '', host: '', hostname: '', port: '', pathname: '',
        search: '', hash: '', origin: '',
        // ownerDocument — lazily returns the global document so React's event setup doesn't crash
        get ownerDocument() { return typeof document !== 'undefined' ? document : null; },
        style: {}, dataset: {},
        innerHTML: '',
        parentNode: null, parentElement: null,
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
        appendChild: function(child) {
            if (child) {
                if (typeof child.tagName === 'string') {
                    this.innerHTML += _r_serialize(child);
                } else if (child.nodeType === 3) {
                    this.innerHTML += _r_esc(child.nodeValue || '');
                } else if (typeof child === 'string') {
                    this.innerHTML += child;
                }
                if (child && typeof child === 'object') {
                    child.parentNode    = this;
                    child.parentElement = this;
                }
            }
            return child;
        },
        prepend: function(child) {
            var s = typeof child === 'string' ? child : _r_serialize(child);
            this.innerHTML = s + this.innerHTML;
        },
        append: function(child) {
            if (typeof child === 'string') this.innerHTML += child;
            else this.appendChild(child);
        },
        insertBefore:    function(n)    { return this.appendChild(n); },
        removeChild:     function(c)    { return c; },
        replaceChild:    function(n, o) { this.appendChild(n); return o; },
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
        insertAdjacentHTML:    function(pos, html) { this.innerHTML += html; },
        insertAdjacentElement: function(pos, el)   { return this.appendChild(el); },
        insertAdjacentText:    function(pos, text) { this.innerHTML += _r_esc(text); },
        hasChildNodes: function() { return this.innerHTML.length > 0; },
        normalize: function() {},
        before: function() {}, after: function() {}, remove: function() {}, replaceWith: function() {},
        requestPointerLock: function() {},
        animate: function() { return { finished: Promise.resolve(), cancel: function(){} }; }
    };
    Object.defineProperty(el, 'textContent', {
        get: function() {
            return el.innerHTML.replace(/<[^>]*>/g, '');
        },
        set: function(v) {
            el.innerHTML = _r_esc(v == null ? '' : String(v));
        },
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
document.createElementNS  = function(ns, tag) { return _r_el(tag); };
document.createTextNode   = function(t) { return {nodeType:3, nodeValue:String(t), textContent:String(t)}; };
document.createComment    = function(t) { return {nodeType:8, nodeValue:t}; };
document.createDocumentFragment = function() { return _r_el('div'); };
document.createRange      = function() {
    return {
        selectNodeContents: function() {},
        toString: function() { return ''; },
        createContextualFragment: function(html) { var d=_r_el('div'); d.innerHTML=html; return d; }
    };
};
document.createEvent = function() { return {initEvent:function(){}, type:'', bubbles:false, cancelable:false}; };
document.nodeType = 9; // DOCUMENT_NODE — needed by React's event-listener setup

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
    // Bundlers (webpack GA snippet) do getElementsByTagName('script')[0].parentNode
    // to find an insertion point. Return a script-like element parented to head.
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
// Stub for scripts that read document.currentScript.src (webpack publicPath) or
// document.currentScript.parentElement (SvelteKit init).
document.currentScript = {
    src: '__HREF__', type: 'text/javascript', nodeType: 1, tagName: 'SCRIPT',
    parentElement: document.head, parentNode: document.head,
    getAttribute: function(n) { return n === 'src' ? this.src : n === 'type' ? this.type : null; },
    setAttribute: function() {}, hasAttribute: function(n) { return n === 'src' || n === 'type'; }
};

// ─── window ──────────────────────────────────────────────────────────────────

(function() {
    var _u = _r_parse_url("__HREF__");
    window.location = {
        href:     _u.href,     protocol: _u.protocol, host:     _u.host,
        hostname: _u.hostname, port:     _u.port,     pathname: _u.pathname,
        search:   _u.search,   hash:     _u.hash,     origin:   _u.origin,
        assign: function() {}, replace: function() {}, reload: function() {},
        toString: function() { return this.href; }
    };
})();
window.navigator = {
    userAgent: 'rakers/0.1.0', appName: 'rakers', appVersion: '0.1.0',
    language: 'en-US', languages: ['en-US', 'en'],
    platform: 'Linux', vendor: '',
    onLine: false, cookieEnabled: false,
    javaEnabled: function() { return false; }
};
window.screen = {width:1920, height:1080, availWidth:1920, availHeight:1080, colorDepth:24};
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
// Collect deferred callbacks so we can flush them after scripts finish.
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
window.MutationObserver     = function(cb) { this.observe=function(){}; this.disconnect=function(){}; this.takeRecords=function(){return [];}; };
window.ResizeObserver       = function(cb) { this.observe=function(){}; this.disconnect=function(){}; this.unobserve=function(){}; };
window.IntersectionObserver = function(cb) { this.observe=function(){}; this.disconnect=function(){}; this.unobserve=function(){}; };
window.PerformanceObserver  = function(cb) { this.observe=function(){}; this.disconnect=function(){}; };
window.CustomEvent  = function(type, init) { this.type=type; this.detail=init&&init.detail||null; this.bubbles=false; this.cancelable=false; };
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
// Analytics stubs — prevents crashes when GA/GTM scripts fail to load
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
window.Document     = function() {};
window.DocumentFragment = function() {};
window.Window       = function() {};
window.ShadowRoot   = function() {};
window.devicePixelRatio = 1;
window.innerWidth  = 1920; window.innerHeight = 1080;
window.outerWidth  = 1920; window.outerHeight = 1080;
window.pageXOffset = 0;    window.pageYOffset = 0;
window.scrollX     = 0;    window.scrollY     = 0;
window.scrollTo    = function() {}; window.scroll   = function() {}; window.scrollBy = function() {};
window.print       = function() {}; window.focus    = function() {}; window.blur     = function() {};
