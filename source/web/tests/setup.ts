import "@testing-library/jest-dom";

// Radix UI primitives (e.g. Select) call Pointer Capture + scrollIntoView
// APIs that jsdom does not implement. Polyfill them as no-ops so the
// components can be driven with userEvent in tests.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}
