#ifndef ANDROID_RAW_PICK_H
#define ANDROID_RAW_PICK_H

extern "C++" {
    // Launch our own ACTION_OPEN_DOCUMENT and report the picker's URI as the
    // raw Java string, before any QUrl exists. Diagnostic only — see
    // android_raw_pick.cpp for why this exists and when to delete it.
    //
    // Returns false if the intent could not be launched at all. The outcome of a
    // launched pick arrives asynchronously through raw_document_pick_result_c().
    bool start_raw_document_pick();
}

#endif
