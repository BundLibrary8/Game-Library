# Gamelib --- Initial Application Issue Register

**Document type:** QA findings and remediation backlog\
**Project:** Gamelib\
**Status:** Initial review; issues identified but not yet investigated
or fixed\
**Audience:** AI coding agents and developers working in the Gamelib
repository

------------------------------------------------------------------------

## Instructions for the implementing agent

Use this document as an initial issue backlog, not as permission to make
broad or destructive changes immediately.

1.  **Inspect the existing repository and application first.** Identify
    the relevant screens, components, services, integrations,
    persistence mechanisms and existing design conventions before
    editing.
2.  **Verify each issue against the current code and running
    application.** Some findings are observations from manual testing
    and may have multiple possible causes.
3.  **Preserve existing functionality** unless an issue explicitly calls
    for removing or changing it. Avoid replacing whole screens or
    systems when a targeted fix is sufficient.
4.  **Group related work sensibly.** In particular, treat the
    settings/accessibility findings and general design findings as
    related, but do not lose their individual acceptance criteria.
5.  **Do not expose secrets.** Never hardcode private API credentials
    into a distributed desktop executable, commit secrets, or move
    sensitive credentials into client-side code. Investigate secure
    server-side or provider-supported authentication patterns.
6.  **Do not assume every advanced feature should be deleted.** Prefer
    progressive disclosure: keep advanced or developer functionality
    available where useful, but keep it out of the ordinary user's way.
7.  **Update this document or the repository's issue tracker** as
    findings are verified, refined, implemented and tested. Do not mark
    an issue resolved without testing it.
8.  **Report uncertainties and dependencies.** If an issue depends on a
    third-party API, platform restriction, account permission or product
    decision, document that rather than inventing a solution.

### Priority definitions

-   **Critical:** Security exposure, data loss, or a core application
    failure affecting most users.
-   **High:** A major user journey or important feature is broken or
    significantly confusing.
-   **Medium:** Noticeable usability, presentation or feature-quality
    problem with a workaround.
-   **Low:** Cosmetic, optional or relatively minor improvement.

Priorities below are provisional and should be adjusted after
investigation.

------------------------------------------------------------------------

# Issue register

## A. Application startup and first-run experience

### GAM-001 --- Missing application icon

**Category:** Branding / Desktop packaging\
**Priority:** Low\
**Status:** Identified

**Observed:** The executable does not appear to have an application icon
before launch. The taskbar icon appears once the application is running.

**Expected:** - Provide a proper Gamelib icon for the executable and
installed application. - Ensure the icon is consistent across the
executable, shortcuts, taskbar and application window. - Check the
packaging/build configuration and operating-system icon caching where
relevant.

**Acceptance criteria:** - The built executable displays the intended
icon in Windows Explorer. - Shortcuts and the running application
display the same icon. - The icon remains present after a clean build
and installation.

### GAM-002 --- Returning users are sent to the login page

**Category:** Authentication / Startup\
**Priority:** High\
**Status:** Identified

**Observed:** Even after logging in previously, the application opens on
a login page with a "Continue as" button.

**Expected:** - Restore a valid existing session automatically. - Send
returning authenticated users directly into the application. - Show the
login flow only when the user is signed out, the session has expired, or
reauthentication is genuinely required. - Handle expired or invalid
sessions clearly without getting stuck in a login loop.

**Acceptance criteria:** - Restarting the application with a valid
persisted session bypasses the login page. - Signing out still returns
the user to the authentication flow. - Expired sessions are handled
safely and predictably. - Session persistence follows the application's
security requirements.

### GAM-003 --- Library is the default landing page

**Category:** Navigation / Product experience\
**Priority:** Medium\
**Status:** Identified

**Observed:** The first screen shown after opening the application is
the library. This does not feel like a particularly suitable
introduction or home experience.

**Expected:** - Investigate a dedicated home/dashboard screen as the
default landing page. - Consider a useful overview such as recently
played games, recent activity, recommendations, library shortcuts and
discovery. - Keep the library easy to access. - Avoid creating a
dashboard that is merely decorative or duplicates information without
adding value.

**Acceptance criteria:** - The default landing experience gives users a
useful overview and clear next actions. - Users can navigate to the
library easily. - The landing page works for both new and returning
users.

------------------------------------------------------------------------

## B. Game library data and integrations

### GAM-004 --- Missing playtime for non-Steam games

**Category:** Integrations / Game data\
**Priority:** High\
**Status:** Identified

**Observed:** Games such as Fortnite and Valorant do not show playtime.

**Expected:** - Investigate available official APIs and platform
integrations for retrieving playtime. - Where external playtime data is
unavailable, investigate reliable local playtime tracking. - Clearly
distinguish tracked playtime from imported/provider-reported playtime if
their accuracy or coverage differs. - Avoid presenting guessed or
fabricated playtime.

**Acceptance criteria:** - Supported games display playtime where a
reliable source exists. - Games without an available source have a clear
and consistent empty/unknown state. - If local tracking is implemented,
it handles application restarts and duplicate processes sensibly. -
Tracking limitations and permissions are documented.

### GAM-005 --- Missing or inconsistent game cover art

**Category:** Game metadata / Presentation\
**Priority:** Medium\
**Status:** Identified

**Observed:** Valorant has no cover art, while Steam games generally do.

**Expected:** - Improve artwork retrieval so non-Steam games also
receive suitable artwork. - Investigate metadata providers, matching
logic, caching and fallback artwork. - Handle missing, ambiguous or
unavailable artwork gracefully. - Keep artwork presentation consistent
across the library.

**Acceptance criteria:** - Valorant and other supported non-Steam titles
can display suitable artwork where a reliable match exists. - Missing
artwork does not leave broken images or visually broken cards. - Artwork
is cached appropriately and does not cause excessive repeated
requests. - Incorrect matches can be corrected or safely fall back to a
placeholder.

### GAM-006 --- Steam account integration is incomplete and requires manual API-key setup

**Category:** Integrations / Onboarding\
**Priority:** High\
**Status:** Identified

**Observed:** - Steam games owned by the user but not currently
installed are not discovered. - There is no apparent Steam account login
flow. - The application asks users to provide a Steam Web API key.

**Expected:** - Investigate a proper Steam
account-linking/authentication experience. - Retrieve the user's owned
games, including games that are not installed, where supported by
Steam's APIs and account privacy settings. - Distinguish owned games
from installed games in the library. - Make synchronisation
understandable and easy to use. - Avoid requiring ordinary users to
obtain and paste their own API key if the integration can be implemented
securely another way.

**Security and implementation note:** Do not embed a private Steam API
key in the distributed client or assume that hiding a key in the
application makes it secure. Investigate the appropriate server-side
architecture, credential handling and Steam-supported authentication
mechanisms. Document any API or account-privacy limitations.

**Acceptance criteria:** - A user can link their Steam account through a
clear supported flow. - Owned and installed status are represented
separately. - Uninstalled owned games can appear when the account/API
makes that information available. - The user is not asked to manually
supply a developer API key in the normal onboarding flow, unless a
documented technical constraint makes this unavoidable. - Sync failures
and private-library limitations are communicated clearly.

------------------------------------------------------------------------

## C. Recommendations and game discovery

### GAM-007 --- Recommendations only focus on games the user already owns

**Category:** Recommendations / Discovery\
**Priority:** Medium\
**Status:** Identified

**Observed:** "Recommended for you" mainly shows games the user owns but
has not played much.

**Expected:** - Expand recommendations to include games the user does
not own. - Consider preferences, play history, genres, similar games and
other relevant signals. - Distinguish owned games from games available
to discover or purchase. - Avoid presenting a list of unplayed owned
games as the entirety of personalised recommendations.

**Acceptance criteria:** - Recommendations can include relevant unowned
games. - Owned and unowned recommendations are clearly
distinguishable. - Users have a useful route to learn more about an
unowned recommendation. - Recommendations remain relevant and do not
rely on unsupported assumptions about the user's preferences.

------------------------------------------------------------------------

## D. Couch mode

### GAM-008 --- Couch mode library, collections and search navigation do not work properly

**Category:** Bug / Navigation\
**Priority:** High\
**Status:** Identified

**Observed:** In couch mode, navigation involving Library, Collections
and Search does not work correctly. The user can navigate using Q/E, but
the corresponding interface/navigation behaviour is not working as
expected.

**Expected:** - All visible navigation options should work. - Q/E
keyboard navigation should continue to work if it is an intended control
method. - Search, library and collections should be reachable and usable
without relying exclusively on Q/E.

**Acceptance criteria:** - Library, Collections and Search can each be
opened through their visible controls. - Navigation state and selected
tab remain consistent. - Existing Q/E behaviour is preserved unless
there is a deliberate, documented redesign. - The relevant screens can
be used after navigating between them.

### GAM-009 --- Mouse support in couch mode is severely limited

**Category:** Bug / Input support\
**Priority:** High\
**Status:** Identified

**Observed:** - Mouse interaction barely works in couch mode. - Once in
Search, the user cannot properly change tabs because navigation relies
on Q/E. - Top navigation buttons do not work.

**Expected:** - Provide working mouse interaction throughout couch mode,
alongside keyboard and controller input where supported. - Make visible
buttons and tabs genuinely interactive. - Do not force users to use Q/E
to perform actions that are visibly presented as clickable controls.

**Acceptance criteria:** - Top navigation buttons respond to mouse
clicks. - Tabs can be changed using the mouse. - Search can be entered,
used and exited with the mouse. - Mouse, keyboard and controller
navigation do not leave the interface in inconsistent states. - Test the
full couch-mode journey, not just individual buttons.

------------------------------------------------------------------------

## E. Readability, typography and visual design

### GAM-010 --- Text appeared too small during initial review

**Category:** Accessibility / Presentation\
**Priority:** Medium\
**Status:** Needs clarification / partially explained

**Observed:** A lot of text initially appeared too small and was
difficult to read. The user later discovered that the text-size setting
had been set to 75%, which explains at least part of the problem.

**Expected:** - Verify the default text-size setting and ensure it is
sensible. - Review text hierarchy and readability at the default
setting. - Do not treat the user's 75% setting as proof that the default
typography is defective. - Consider whether the UI remains readable
across supported display sizes and scaling settings.

**Acceptance criteria:** - Default text sizing is comfortable to read. -
Important labels and secondary information have a clear hierarchy. - The
application remains usable at supported accessibility scaling levels.

### GAM-011 --- Font selection and typography need improvement

**Category:** Visual design\
**Priority:** Medium\
**Status:** Identified

**Observed:** The overall font choices and typography do not look
particularly good.

**Expected:** - Review font selection, weights, sizes, line heights and
spacing. - Establish a consistent typography system across the
application. - Keep the visual identity cohesive rather than making
isolated font changes on individual screens.

**Acceptance criteria:** - Typography is consistent across major screens
and components. - Headings, labels, body text and secondary text are
visually distinguishable. - Font choices remain readable at different
accessibility text sizes.

### GAM-012 --- General UI needs visual refinement

**Category:** UI/UX / Visual design\
**Priority:** Medium\
**Status:** Identified

**Observed:** The overall interface does not feel sufficiently polished
and needs general work.

**Expected:** Review the application holistically, including: - Layout
and spacing. - Component consistency. - Visual hierarchy. - Navigation
clarity. - Alignment and sizing. - Empty, loading and error states. -
Consistency between standard mode and couch mode.

**Acceptance criteria:** - Major screens follow a coherent design
system. - Components behave and look consistently. - Visual changes do
not break existing functionality or accessibility. - The result feels
like one cohesive application rather than a collection of unrelated
screens.

------------------------------------------------------------------------

## F. Settings and accessibility

### GAM-013 --- Settings are cluttered and difficult to understand

**Category:** UI/UX / Settings\
**Priority:** High\
**Status:** Identified

**Observed:** The settings page contains a lot of text and buttons, but
the information is not presented concisely. It is confusing to look at
and difficult to understand.

**Expected:** - Reorganise settings into clear, meaningful categories. -
Use concise labels and descriptions. - Make common settings easy to
find. - Reduce unnecessary buttons and explanatory text. - Use sensible
controls for each setting. - Keep advanced configuration separate from
everyday settings.

**Acceptance criteria:** - A normal user can understand what common
settings do without reading lengthy explanations. - Related settings are
grouped together. - The page has a clear visual hierarchy. - Advanced
options do not overwhelm the default settings experience.

### GAM-014 --- API-key configuration is exposed to ordinary users

**Category:** Security / Settings / Product design\
**Priority:** High\
**Status:** Identified; implementation requires investigation

**Observed:** Settings include an API-key input. The user considers this
inappropriate for a public consumer application, where ordinary users
should not have to configure developer credentials.

**Expected:** - Identify which integration the API-key field belongs to
and why it exists. - Determine whether the integration can use
application-managed credentials or a different supported authentication
flow. - Remove unnecessary API-key configuration from ordinary user
settings. - Keep developer-only configuration in an appropriate
development/debug area if it is genuinely needed.

**Security note:** Application-managed credentials must be handled
securely. Do not ship private API secrets inside the desktop executable,
expose them in client-side code, or commit them to the repository. Use a
secure backend or another provider-supported approach where appropriate.
Some integrations may legitimately require user authorisation; do not
replace one insecure pattern with another.

**Acceptance criteria:** - The normal user does not need to understand
or provide developer API keys for routine use where a secure alternative
exists. - No private credentials are embedded in the distributed
application. - Any unavoidable user-provided credential has a clear
reason, secure storage and concise explanation. - Developer
configuration is separated from normal settings.

### GAM-015 --- Accessibility text-size slider causes layout glitches

**Category:** Bug / Accessibility\
**Priority:** High\
**Status:** Identified

**Observed:** Moving the text-size slider causes the interface to shift
around. The layout glitches, and the slider itself can move or become
difficult to control.

**Expected:** - Make text-size adjustment stable. - Consider a
preview-and-save interaction rather than continuously reflowing the
entire interface while the slider is being dragged. - Provide Save/Apply
and Cancel/Reset controls. - Ensure the slider stays in a stable
position and remains usable while previewing a change.

**Acceptance criteria:** - Moving the slider does not make the control
jump, move away or become unusable. - Previewing a size does not cause
overlapping or broken layouts. - Users can apply or cancel a change. -
The chosen setting persists after saving and restarting the
application. - The control remains usable with mouse and supported
keyboard input.

### GAM-016 --- Sidebar does not fit at larger text sizes

**Category:** Bug / Accessibility / Responsive layout\
**Priority:** Medium\
**Status:** Identified

**Observed:** At higher text-size settings, the sidebar no longer fits
properly.

**Expected:** - Make the sidebar responsive to accessibility scaling. -
Allow suitable wrapping, resizing, scrolling or adaptive layout
behaviour. - Prevent navigation labels and controls from clipping or
overlapping.

**Acceptance criteria:** - Sidebar navigation remains usable at
supported larger text sizes. - No essential navigation item becomes
inaccessible. - Layout remains stable when changing between text-size
settings.

------------------------------------------------------------------------

## G. Discord integration

### GAM-017 --- Discord Rich Presence may be unnecessary or redundant

**Category:** Integrations / Product scope\
**Priority:** Low\
**Status:** Needs investigation

**Observed:** Gamelib has Discord Rich Presence, but Discord already has
automatic game activity detection. The user questions whether the
additional integration is worthwhile.

**Expected:** - Investigate what Gamelib's Rich Presence provides beyond
Discord's built-in activity detection. - Determine whether it adds
meaningful information or functionality. - If it is redundant, consider
removing it or making it optional. - If it provides a distinct benefit,
explain that benefit clearly and keep the integration unobtrusive.

**Acceptance criteria:** - There is a clear product reason for retaining
the integration. - Unnecessary background activity and configuration are
avoided. - Any retained integration behaves reliably and can be
controlled by the user where appropriate.

------------------------------------------------------------------------

## H. Product complexity and design philosophy

### GAM-018 --- Too much technical information is exposed to normal users

**Category:** Product design / UI/UX\
**Priority:** High\
**Status:** Identified

**Observed:** Too many parts of the application expose information,
settings or controls that an ordinary user should not need to see.
Examples include API-key configuration, excessive settings and technical
integration details.

**Expected:** Adopt progressive disclosure:

-   **Basic:** Common settings and features that most users need.
-   **Advanced:** Optional configuration for experienced users.
-   **Developer / Diagnostics:** Debugging, technical details,
    integration diagnostics and internal tools.

Keep advanced functionality available where it is useful, but do not
make it part of the normal user's primary experience.

**Acceptance criteria:** - Ordinary users can complete common tasks
without encountering unnecessary technical configuration. - Advanced
settings are clearly separated and discoverable when needed. - Developer
and diagnostic tools remain available to developers without cluttering
the consumer interface. - Hiding advanced options does not remove
required functionality.

### GAM-019 --- Establish a consumer-first product experience

**Category:** Product direction / UX principle\
**Priority:** High\
**Status:** Design requirement

**Context:** This is a broader product requirement synthesised from the
user's observations, rather than a single isolated bug.

**Requirement:** Gamelib should feel like a polished consumer gaming
platform, not a developer tool or internal administration dashboard.

A normal user should generally be able to: 1. Install and open the
application. 2. Sign in, or have an existing session restored. 3. Import
and synchronise their games. 4. Browse their library. 5. Discover games
they may enjoy. 6. Launch games. 7. Manage collections and preferences.

They should not need to understand APIs, authentication mechanisms,
integration configuration or developer terminology to use the core
product.

**Acceptance criteria:** - Core user journeys are straightforward and
clearly presented. - Technical details are hidden unless relevant. -
Common actions are easy to discover. - The interface remains
approachable without removing useful advanced capabilities.

------------------------------------------------------------------------

# Cross-issue themes and suggested workstreams

These workstreams group related issues for planning. They are not a
substitute for the individual issue IDs and acceptance criteria.

  -----------------------------------------------------------------------
  Workstream              Related issues          Goal
  ----------------------- ----------------------- -----------------------
  Startup and onboarding  GAM-001, GAM-002,       Make installation,
                          GAM-003                 startup and
                                                  returning-user
                                                  experience feel
                                                  complete

  Game data and           GAM-004, GAM-005,       Improve coverage and
  integrations            GAM-006                 reliability of game
                                                  ownership, playtime and
                                                  artwork

  Discovery               GAM-007                 Make recommendations
                                                  useful beyond games
                                                  already owned

  Couch mode              GAM-008, GAM-009        Make all intended input
                                                  methods work
                                                  consistently

  Design system           GAM-010, GAM-011,       Improve readability,
                          GAM-012                 typography and overall
                                                  polish

  Settings and            GAM-013, GAM-014,       Simplify configuration
  accessibility           GAM-015, GAM-016        and make accessibility
                                                  controls reliable

  Optional integrations   GAM-017                 Review whether Discord
                                                  Rich Presence provides
                                                  enough value

  Consumer-first          GAM-018, GAM-019        Keep technical
  experience                                      complexity out of the
                                                  normal user journey
  -----------------------------------------------------------------------

## Suggested investigation and implementation order

This is a suggested order, not a final commitment. Reassess after
inspecting the codebase and identifying dependencies.

1.  **Audit the current application and architecture.** Map each issue
    to the relevant code and verify the behaviour.
2.  **Address security and authentication concerns.** Investigate
    GAM-014 and GAM-002 before making credential or session changes.
3.  **Fix broken core interactions.** Prioritise GAM-008, GAM-009 and
    GAM-015.
4.  **Improve game-data foundations.** Investigate GAM-004, GAM-005 and
    GAM-006, including API limitations and credential architecture.
5.  **Simplify settings and establish design conventions.** Work on
    GAM-013, GAM-016, GAM-018 and the design-system issues together.
6.  **Improve the primary user journey.** Address GAM-003 and GAM-007.
7.  **Review optional features and polish.** Address GAM-001, GAM-011,
    GAM-012 and GAM-017.

## Important clarification from the manual review

The initial complaint that text looked too small was partly explained by
the text-size setting being at **75%**. Do not assume the default font
size is too small based solely on that observation. The separate slider
glitch and sidebar overflow at larger sizes are still valid reported
issues and should be tested independently.

## Definition of done

An issue should only be marked complete when:

-   The relevant behaviour has been reproduced or the underlying
    implementation has been inspected.
-   A targeted fix has been implemented.
-   Relevant existing behaviour has been regression-tested.
-   The acceptance criteria have been checked.
-   Any limitations or unresolved dependencies have been documented.
-   The issue register or repository issue tracker has been updated with
    the result.

------------------------------------------------------------------------

**End of initial Gamelib issue register.**
