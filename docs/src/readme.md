# Introduction

In this document I'm going to layout the current state of changelog management for multi-language
monorepos. The conclusion may be that there already exists a good solution and I don't need to spend
time working on this project.

## What do I want?

Building a successful open source project is hard. Managing code, contributors, releases,
documentation is a lot of work. There is also no magic solution that works for everyone. While
working on `remirror` one area that constantly bothered me was how to manage changes. Initially
since it was a NodeJS project I used `lerna` and `conventional commits` to manage releases. This
worked well for a while but as the project grew it became more difficult to manage.

At that time I stumbled across changesets which became my goto change management tool for all my
projects. It's a great solution, but not perfect.

The things I think it got right are:

- Creating a PR that can manage the release of multiple packages across a workspace.
- Seamlessly handling semver compatible bumps across the workspace.
- Automatically updating the changelog for each package.
- Automated releases to npm and github.

The things that I struggled with are the following:

- It's not multi-language. It's only for NodeJS projects.
- It uses changeset files to manage releases. This creates some contributor friction as they need to
  learn how to set it up and use it before submitting semver impacting changes.
- Configuration was sometimes confusing and limited.
- Overly granular and creates a lot of noise in the releases tab. Sometimes I don't want a package
  to add a tag when released. This is possible to accomplish but requires custom code to accomplish.
- No automated dates added to changelogs (tiny)

Over time my attention turned to auto.

I decided against migrating to auto, but the features I loved were:

- Changelogs created from the pull request message. I think this is great since it's more visible
  than a changeset file, so typos are less likely. Also it can be edited after the fact. Imagine a
  changelog that would update when the PR message was updated. That would be awesome.
- [Plugin architecture](https://intuit.github.io/auto/docs/plugins/writing-plugins) which will be
  explored later in this document. `auto` allows for publishing to multiple platforms.

## Some example changelogs

- [vscode](https://code.visualstudio.com/updates/v1_75) changes are so well documented they should
  become industry standard. I actually enjoy reading them.
- [linear](https://linear.app/changelog) treats their changelogs as part of the product which makes
  a lot of sense.

## This solution

`monochange` aims to be a solution to changelog management for all my future projects.

It should:

- support multiple languages
- automatically bump versions across workspaces
- understand dependencies between packages, even when cross language dependencies (rust ffi in deno)
- implement a way to publish deno packages to `deno.land/x` (creating a seperate repo for each
  package that is automatically updated when publishing)
- plugin architecture to implement language specific publishing

## Some alternatives

TODO: Add reasons why they are not suitable for my use case.

- [GitHub - pksunkara/cargo-workspaces: A tool for managing cargo workspaces and their crates, inspired by lerna](https://github.com/pksunkara/cargo-workspaces)
  especially
  [version.rs](https://github.com/pksunkara/cargo-workspaces/blob/master/cargo-workspaces/src/utils/version.rs)
- [GitHub - changesets/changesets: 🦋 A way to manage your versioning and changelogs with a focus on monorepos](https://github.com/changesets/changesets)
- [GitHub - intuit/auto: Generate releases based on semantic version labels on pull requests.](https://github.com/intuit/auto/)
- [GitHub - jbolda/covector: Transparent and flexible change management for publishing packages and assets.](https://github.com/jbolda/covector)
- [cargo-dist](https://github.com/axodotdev/cargo-dist)
