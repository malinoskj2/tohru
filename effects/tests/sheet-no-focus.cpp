#include "../sheet/NoFocusRestore.hpp"

#include <array>
#include <cassert>
#include <iostream>
#include <optional>

// The production helper uses only the pinned COverridableVar public operations.
// Model the four optional priority slots, including lower-policy/default fallback.
struct SNoFocus {
    std::array<std::optional<bool>, 4> slots;
    bool fallback = false;
    bool hasValue() const {
        for (const auto& slot : slots) if (slot) return true;
        return false;
    }
    int getPriority() const {
        for (int i = 3; i >= 0; --i) if (slots[i]) return i;
        assert(false);
        return -1;
    }
    bool value() const { return *slots[getPriority()]; }
    void set(bool value, int priority) { slots[priority] = value; }
    void matchOptional(std::optional<bool> value, int priority) { slots[priority] = value; }
};

int main() {
    for (std::optional<bool> prior : {std::optional<bool>{}, {false}, {true}}) {
        for (std::optional<bool> lower : {std::optional<bool>{}, {false}, {true}}) {
            SNoFocus value{};
            value.slots[2] = lower;
            value.slots[3] = prior;
            Sheet::CNoFocusRestore<3> saved;
            saved.hide(value);
            assert(value.slots[3] == true);
            saved.hide(value); // duplicate hide must not replace the original snapshot
            saved.restore(value);
            assert(value.slots[3] == prior && value.slots[2] == lower);
            saved.restore(value); // cleanup after show is idempotent
            assert(value.slots[3] == prior);
            // A later sheet reuses the state with a fresh snapshot.
            value.slots[3] = !prior.value_or(false);
            const auto next = value.slots[3];
            saved.hide(value);
            saved.restore(value);
            assert(value.slots[3] == next);
        }
        for (std::optional<bool> external : {std::optional<bool>{}, {false}}) {
            SNoFocus value{};
            value.slots[3] = prior;
            Sheet::CNoFocusRestore<3> saved;
            saved.hide(value);
            value.slots[3] = external;
            saved.restore(value);
            assert(value.slots[3] == external);
        }
        // Lower policy changes do not interfere with restoration of our exact slot.
        SNoFocus value{};
        value.slots[3] = prior;
        Sheet::CNoFocusRestore<3> saved;
        saved.hide(value);
        value.slots[2] = true;
        saved.restore(value);
        assert(value.slots[3] == prior && value.slots[2] == true);
    }
    // Cleanup without ever hiding must not touch another owner's setting.
    SNoFocus untouched{};
    untouched.slots[3] = false;
    Sheet::CNoFocusRestore<3> idle;
    idle.restore(untouched);
    assert(untouched.slots[3] == false);
    std::cout << "PASS: sheet noFocus absent/false/true slot restoration, lower policy, repeated cleanup/reuse and observable intervening writes\n";
}
