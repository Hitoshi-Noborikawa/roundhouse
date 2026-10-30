import { test, expect } from '@playwright/test'
import { signIn } from './helpers.js'

// TYPING NOTIFICATIONS: one user types, another sees their name.
//
// The server half is campfire's `TypingNotificationsChannel#start`,
// which broadcasts `{ action: :start, user: current_user.slice(:id,
// :name) }`. Before `ActiveRecord::Base#slice` existed in the runtime,
// that call raised inside the channel action: the frame never went out,
// and the indicator stayed empty with nothing on the page to say why.
//
// TWO USERS, NOT TWO TABS. campfire's client drops a notification whose
// `user.id` is its own (`typing_notifications_controller.js#received`),
// so the cable spec's two-tabs-one-user shape would assert nothing. The
// second user joins through the account's own join link, the way a
// person invited to campfire does.
//
// Unique per run: the archive database persists across runs, and the
// join form refuses an email address that already has an account.
const STAMP = Date.now()
const TYPIST = `Typist ${STAMP}`

// Two sign-ins, a join, two room loads and a cable round trip; the 30s
// default is not a budget for that (see cable.spec.js).
test.setTimeout(90_000)

test("a user typing in a room shows their name to the room's other members", async ({ browser }) => {
  const contextA = await browser.newContext()
  const contextB = await browser.newContext()
  const pageA = await contextA.newPage()
  const pageB = await contextB.newPage()

  try {
    // A is the first user, so an administrator, and the account page
    // shows them the join link.
    await signIn(pageA)
    await pageA.goto('/account/edit')
    const joinUrl = await pageA.locator('#invite_url').inputValue()
    expect(joinUrl, 'the account page shows a join link').toContain('/join/')

    // B joins. The join form is `users/new`, the same three fields the
    // first-run form has; landing is the signed-in layout's sidebar.
    await pageB.goto(new URL(joinUrl).pathname)
    await pageB.locator('#user_name').fill(TYPIST)
    await pageB.locator('#user_email_address').fill(`typist-${STAMP}@example.com`)
    await pageB.locator('#user_password').fill('secret123456')
    await pageB.locator('button[type="submit"]').click()
    await expect(pageB.locator('#user_sidebar')).toBeAttached({ timeout: 15_000 })

    // Room 1 is the first-run room, an open room every user belongs to.
    await pageA.goto('/rooms/1')
    await pageB.goto('/rooms/1')

    // Typing only reaches A once A's subscription is confirmed. The
    // room stream's `connected` attribute shares the consumer the
    // typing channel subscribes on, so it is the nearest observable
    // "cable is up" signal on the page.
    for (const [name, page] of [['A', pageA], ['B', pageB]]) {
      await expect(
        page.locator('turbo-cable-stream-source[connected]').first(),
        `tab ${name} has a live cable connection`,
      ).toBeAttached({ timeout: 20_000 })
    }

    // Typed keys, not `fill()`: the editor is a custom element, and the
    // `lexxy:change` event that drives `typing-notifications#start`
    // fires from real input (see cable.spec.js).
    await pageB
      .locator('lexxy-editor#message_body .lexxy-editor__content, trix-editor#message_body')
      .first()
      .click()
    await pageB.keyboard.type('hello')

    const indicator = pageA.locator('[data-typing-notifications-target="indicator"]')
    await expect(
      pageA.locator('[data-typing-notifications-target="author"]'),
      "A sees B's name in the typing indicator",
    ).toHaveText(TYPIST, { timeout: 15_000 })
    await expect(indicator, 'the indicator is shown').toHaveClass(/typing-indicator--active/)
  } finally {
    await contextA.close()
    await contextB.close()
  }
})
