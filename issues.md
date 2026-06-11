# Issues

To resolve these issues, BREAKING CHANGES ARE EXPECTED AND ALLOWED!

## 1. Homebrew lots of dependencies:
When installing with homebrew:

==> Installing xelandernt/skilly/skilly dependency: python@3.14

Why is python3.14 a dependency of skilly?

Why is llvm a dependency of skilly too? Can we reduce this it is very large?

## 2. Scan features:

When using skilly scan i would be able to exclude specific dependency groups and specific extras.
Alternatively I would also like explicity include only specific depdency groups or specific extras.


## 3. Handle multiple directories feature:

When running `skilly list` I would like to see multiple tabs for all possible skilly directories
(`.agents/skills`, `~/.claude/skills`, `.codex/skills`, `~/.copilot/skills`, etc.)

Tabs that have no skills in them should be greyed out. When running `skilly list` you should always be brought 
to the tab which has some skills, if there are no skilly an any tabs then just write a message.

I should be able to switch between tabs using `Tab`. 

Tabs should have appropriate color coding based on the directory.

It should work in a similar way for other commands such as `skilly download` and `skilly scan` and `skilly skillsmp search` etc.
I should be able to switch tabs and install the skills to different directories.

If there are any open questions, let me know!
