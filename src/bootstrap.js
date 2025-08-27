// rakers browser-globals bootstrap.
// Evaluated before every user script. __HREF__ is replaced by Rust with the page URL.

var window = globalThis;
var self   = window;

// ─── Element factory ────────────────────────────────────────────────────────

function _r_el(tag) {
    tag = (tag || 'DIV').toUpperCase();
    return {
        tagName: tag, nodeType: 1,
        id: '', className: '', name: '', type: '', value: '',
        href: '', src: '', alt: '', placeholder: '',
        style: {}, dataset: {},
        innerHTML: '', textContent: '',
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
            else if (n === 'href')  this.href  = v;
            else if (n === 'src')   this.src   = v;
            else if (n === 'type')  this.type  = v;
            else if (n === 'value') this.value = v;
            else if (n === 'name')  this.name  = v;
        },
        getAttribute: function(n) {
            if (n === 'id')    return this.id    || null;
            if (n === 'class') return this.className || null;
            if (n === 'href')  return this.href  || null;
            if (n === 'src')   return this.src   || null;
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

document.getElementById = function(id) {
    if (!_r_reg[id]) { var e = _r_el('div'); e.id = id; _r_reg[id] = e; }
    return _r_reg[id];
};
document.getElementsByClassName = function() { return []; };
document.getElementsByTagName   = function() { return []; };
document.getElementsByName      = function() { return []; };
document.querySelector = function(sel) {
    if (!sel) return null;
    var m = sel.match(/^#([\w-]+)$/);
    if (m) return document.getElementById(m[1]);
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

// ─── window ──────────────────────────────────────────────────────────────────

window.location = {
    href: "__HREF__",
    hostname: '', pathname: '/', search: '', hash: '', protocol: 'https:', port: '', origin: '',
    assign: function() {}, replace: function() {}, reload: function() {},
    toString: function() { return this.href; }
};
window.navigator = {
    userAgent: 'rakers/0.1.0', appName: 'rakers', appVersion: '0.1.0',
    language: 'en-US', languages: ['en-US', 'en'],
    platform: 'Linux', vendor: '',
    onLine: false, cookieEnabled: false,
    javaEnabled: function() { return false; }
};
window.screen = {width:1920, height:1080, availWidth:1920, availHeight:1080, colorDepth:24};
window.history = {
    length: 1, scrollRestoration: 'auto',
    pushState:    function() {}, replaceState: function() {},
    back:         function() {}, forward:      function() {}, go: function() {}
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
window.setTimeout            = function(fn, delay) { return 0; };
window.clearTimeout          = function(id) {};
window.setInterval           = function(fn, delay) { return 0; };
window.clearInterval         = function(id) {};
window.requestAnimationFrame = function(fn) { return 0; };
window.cancelAnimationFrame  = function(id) {};
window.queueMicrotask        = function(fn) {};
window.alert   = function(msg) {};
window.confirm = function(msg) { return false; };
window.prompt  = function(msg, def) { return null; };
window.open    = function() { return null; };
window.close   = function() {};
window.postMessage     = function() {};
window.fetch           = function() { return Promise.reject(new Error('fetch is not available in rakers')); };
window.XMLHttpRequest  = function() {
    this.readyState=0; this.status=0; this.statusText=''; this.responseText=''; this.responseXML=null;
    this.onreadystatechange=null; this.onload=null; this.onerror=null; this.onprogress=null;
    this.open=function(){}; this.send=function(){}; this.abort=function(){};
    this.setRequestHeader=function(){}; this.getResponseHeader=function(){return null;};
    this.getAllResponseHeaders=function(){return '';};
    this.addEventListener=function(){}; this.removeEventListener=function(){};
};
window.FormData = function() {
    this.append=function(){}; this.delete=function(){};
    this.get=function(){return null;}; this.has=function(){return false;};
    this.set=function(){};
};
window.URL = function(href, base) { this.href=String(href); this.toString=function(){return this.href;}; };
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
window.AbortController = function() { this.signal={aborted:false,addEventListener:function(){}}; this.abort=function(){}; };
window.AbortSignal  = {timeout:function(){return {aborted:false,addEventListener:function(){}};}};
window.TextEncoder  = function() { this.encode=function(s){return new Uint8Array(0);}; };
window.TextDecoder  = function() { this.decode=function(b){return '';}; };
window.crypto       = {getRandomValues:function(a){return a;}, subtle:{}, randomUUID:function(){return '00000000-0000-0000-0000-000000000000';}};
window.CSS          = {supports:function(){return false;}, escape:function(s){return s;}};
window.HTMLElement  = function() {};
window.Element      = function() {};
window.Node         = function() {};
window.EventTarget  = function() {};
window.Document     = function() {};
window.Window       = function() {};
window.devicePixelRatio = 1;
window.innerWidth  = 1920; window.innerHeight = 1080;
window.outerWidth  = 1920; window.outerHeight = 1080;
window.pageXOffset = 0;    window.pageYOffset = 0;
window.scrollX     = 0;    window.scrollY     = 0;
window.scrollTo    = function() {}; window.scroll   = function() {}; window.scrollBy = function() {};
window.print       = function() {}; window.focus    = function() {}; window.blur     = function() {};
