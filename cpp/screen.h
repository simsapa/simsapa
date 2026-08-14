#ifndef SCREEN_H
#define SCREEN_H

#include <QString>

extern "C++" {
    // Hold or release the "keep the screen awake" request for one named holder.
    // The flag is shared by every caller, so it is only cleared once the last
    // holder has released it. See the comment in screen.cpp.
    void keep_screen_on(const QString& holder, bool on);
}

#endif
