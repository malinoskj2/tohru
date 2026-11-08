#pragma once

#include <optional>
#include <utility>

namespace Sheet {
// PRIORITY_SET_PROP is the pinned API's highest slot. Read that slot separately
// from the effective lower-priority/default value; absence must stay absence.
template <auto Priority>
class CNoFocusRestore {
  public:
    template <class Var>
    void hide(Var& value) {
        if (m_active)
            return;
        m_previous = slot(value);
        value.set(true, Priority);
        m_active = true;
    }

    template <class Var>
    void restore(Var& value) {
        if (!std::exchange(m_active, false))
            return;
        // A false value or absent slot is an observable intervening external write.
        // Same-value writes cannot be distinguished: the API has no owner/generation.
        if (slot(value) == std::optional<bool>{true})
            value.matchOptional(m_previous, Priority);
        m_previous.reset();
    }

  private:
    template <class Var>
    static std::optional<bool> slot(const Var& value) {
        if (value.hasValue() && value.getPriority() == Priority)
            return value.value();
        return std::nullopt;
    }

    bool m_active = false;
    std::optional<bool> m_previous;
};
}
