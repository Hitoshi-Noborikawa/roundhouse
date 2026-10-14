//! `#800` remainder: a plain library class's flattened optional
//! keywords, called through a receiver whose type the analyzer could
//! not resolve at all.
//!
//! #814 repaired every receiver shape it could identify: a CONST
//! receiver (`Builder.new`/`Builder.column`), a value typed as an
//! INSTANCE of the owning class (`Ty::SelfInstance`, `b = new` in a
//! class method), and the owner-relative shapes (a receiverless call
//! or `self.` from the class's own methods, inherited). A receiver
//! whose type resolves to none of those — an untyped parameter with no
//! call site to infer from, here — still fell through with NO
//! diagnostic, and Ruby bound the call's whole keyword Hash to the
//! first flattened positional slot.
//!
//! Guessing which class's slot layout applies to an unresolved
//! receiver would repeat the exact silent mis-bind #800 reported, one
//! level up; the honest answer is the same "flattened keyword
//! arguments not supported" refusal #814 already gives a call it CAN
//! see the receiver's class for but cannot safely respell.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

/// `Kwlab::Caller.run`'s `b` parameter has no call site anywhere in
/// this app to infer a type from, so the analyzer leaves it untyped —
/// the receiver shape #814 does not cover. `field`'s name coincides
/// with `Builder#field`, the one flattened method in this fixture.
const SOURCE: &str = r##"module Kwlab
  class Builder
    def field(name, type = nil, null: true, description: nil)
      [name, type, null, description]
    end
  end

  class Caller
    def self.run(b)
      b.field(:a, null: false)
    end
  end
end
"##;

#[test]
fn an_untyped_receivers_keyword_call_is_refused_not_guessed() {
    let (_emitted, errors) = emit_and_run::empty_app()
        .write("app/lib/kwlab/builder.rb", SOURCE)
        .emit(BuildTarget::Ruby);
    assert!(
        errors.iter().any(|e| e.contains("flattened keyword arguments not supported")),
        "an untyped receiver's keyword call to a flattened method must be refused, not silently \
         mis-bound; errors: {errors:#?}"
    );
}

/// The same call through a receiver typed as the class (#814's own
/// coverage) must stay quiet — this commit only widens the refusal to
/// the untyped case, not to every call of a same-named method.
#[test]
fn the_typed_receiver_twin_still_moves_cleanly() {
    const TYPED: &str = r##"module Kwlab
  class Builder
    def field(name, type = nil, null: true, description: nil)
      [name, type, null, description]
    end
  end

  class Caller
    def self.run
      Builder.new.field(:a, null: false)
    end
  end
end
"##;
    let (_emitted, errors) = emit_and_run::empty_app()
        .write("app/lib/kwlab/builder.rb", TYPED)
        .emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
