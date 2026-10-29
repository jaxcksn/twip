---
title: React and Angular
sidebar_position: 8
---

# React and Angular

`@twip/web` depends on no framework. An adapter needs four calls: mount, destroy, `subscribe` and `on`. These are
complete adapters you can copy into your application.

## React

`useTwipState` reads the editor's state through `useSyncExternalStore`, so a component re-renders after every change
and never tears. `useTwipEditor` mounts an editor in an element and destroys it on unmount.

```ts file=../../examples/web/react.ts title="useTwip.ts"
```

```tsx file=../../examples/web/react-toolbar.tsx title="DocumentView.tsx"
```

`getState()` returns the same snapshot until something changes, which is what `useSyncExternalStore` needs.

## Angular

`TwipStateService` turns `subscribe` into a signal and ties listeners to the lifetime of the injector that provides
it. Provide it in the component that mounts the editor.

```ts file=../../examples/web/angular.ts title="twip-state.service.ts"
```

```ts file=../../examples/web/angular-toolbar.ts title="document.component.ts"
```

Mount in `afterNextRender`: the editor needs its container in the DOM and sized.
