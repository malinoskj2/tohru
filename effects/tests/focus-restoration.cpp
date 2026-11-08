#include "../focus/Restoration.hpp"

#include <cassert>
#include <functional>
#include <iostream>
#include <string>

struct SVariable {
    float current;
    float target;
    int config;
    std::function<void()> end;
    SVariable& operator=(float value) { target = value; return *this; }
    void setConfig(int value) { config = value; }
    void setCallbackOnEnd(std::nullptr_t) { end = {}; }
    void warp(bool callback) { current = target; if (callback && end) end(); }
};

using Restore = Focus::SRestoration<float, int>;

static void interrupted(bool shrink, bool outward, bool mapped, bool tiled, bool flash) {
    Restore restore;
    restore.geometry.emplace(Restore::SGeometry{10, 100, 1, 2, shrink});
    SVariable position{outward ? 8.F : 9.F, outward ? 10.F : 7.F, 10, {}};
    SVariable size{outward ? 97.F : 99.F, outward ? 100.F : 95.F, 10, {}};
    SVariable alpha{0.9F, outward ? 1.F : 0.8F, 10, {}};
    bool foreignPositionCalled = false;
    if (shrink) {
        position.end = [&] { foreignPositionCalled = true; };
        size.end = restore.geometry->callback.wrap([] {});
    } else
        position.end = restore.geometry->callback.wrap([] {});
    if (flash) {
        restore.opacity.emplace(Restore::SOpacity{1, 3});
        alpha.end = restore.opacity->callback.wrap([] {});
    }
    int recalcs = 0;
    bool released = false;
    restore.unload(position, size, alpha, mapped, tiled, [&] {
        ++recalcs;
        // Simulate a changed tiled layout while the render-only dip held sizing.
        position = 20;
        size = 200;
    }, [&] {
        // The client-sizing hook must remain held until geometry/opacity are restored.
        assert(position.current == (mapped && tiled ? 20 : 10));
        if (shrink) assert(size.current == (mapped && tiled ? 200 : 100));
        assert(!flash || alpha.current == 1);
        released = true;
    });
    assert(released && recalcs == (mapped && tiled ? 1 : 0));
    assert(position.config == 1 && (!shrink || size.config == 2));
    assert(!flash || (alpha.config == 3 && !alpha.end));
    assert(!restore.geometry && !restore.opacity);
    assert(shrink ? bool(position.end) && !size.end : !position.end);
    assert(!foreignPositionCalled);
}

int main() {
    for (bool shrink : {false, true})
        for (bool outward : {false, true})
            for (bool mapped : {false, true})
                for (bool tiled : {false, true})
                    for (bool flash : {false, true})
                        interrupted(shrink, outward, mapped, tiled, flash);

    // A foreign replacement must survive teardown, without running while restoring.
    Restore restore;
    restore.geometry.emplace(Restore::SGeometry{10, 100, 1, 2, true});
    restore.opacity.emplace(Restore::SOpacity{1, 3});
    SVariable position{9, 9, 10, {}}, size{99, 99, 10, {}}, alpha{0.9F, 0.9F, 10, {}};
    size.end = restore.geometry->callback.wrap([] {});
    alpha.end = restore.opacity->callback.wrap([] {});
    assert(restore.geometry->callback.ownsCallback() && restore.opacity->callback.ownsCallback());
    bool foreignCalled = false;
    size.end = [&] { foreignCalled = true; };
    alpha.end = [&] { foreignCalled = true; };
    assert(!restore.geometry->callback.ownsCallback() && !restore.opacity->callback.ownsCallback());
    restore.unload(position, size, alpha, false, false, [] { assert(false); }, [] {});
    assert(size.end && alpha.end && !foreignCalled);
    assert(position.current == 10 && size.current == 100 && alpha.current == 1);

    // Flash alone is tracked through the outward leg; unrelated geometry/callbacks survive.
    restore.opacity.emplace(Restore::SOpacity{0.7F, 3});
    alpha.end = restore.opacity->callback.wrap([] {});
    position.end = [&] { foreignCalled = true; };
    restore.unload(position, size, alpha, true, true, [] { assert(false); }, [] {});
    assert(alpha.current == 0.7F && alpha.config == 3 && !alpha.end && position.end);
    std::cout << "PASS: focus inward/outward, floating/tiled, mapped/unmapped, slide/shrink/combined/flash, restoration-before-release and foreign callback preservation\n";
}
