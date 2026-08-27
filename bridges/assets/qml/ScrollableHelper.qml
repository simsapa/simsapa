pragma ComponentBehavior: Bound

import QtQuick

/**
 * ScrollableHelper - Reusable component for managing automatic scrolling to bottom
 *
 * Usage:
 * ScrollableHelper {
 *     id: scroll_helper
 *     target_scroll_view: your_scroll_view
 * }
 *
 * Then call: scroll_helper.scroll_to_bottom()
 */
QtObject {
    id: root

    // The ScrollView to control - can be set by the user or dynamically
    property var target_scroll_view: null

    // Internal properties for tracking content height
    property real last_content_height: 0

    // Timer for delayed scrolling to ensure layout completion
    property Timer scroll_timer: Timer {
        interval: 150
        repeat: false
        onTriggered: {
            root.perform_scroll_if_needed();
        }
    }

    property Logger logger: Logger { id: logger }

    // Main function to trigger scroll to bottom
    function scroll_to_bottom() {
        // Only scroll if needed - check if content exceeds view
        scroll_timer.restart();
    }

    // Internal function that performs the actual scroll check and operation
    function perform_scroll_if_needed() {
        if (!target_scroll_view || !target_scroll_view.contentItem) {
            logger.warn("ScrollableHelper: target_scroll_view or contentItem is null");
            return;
        }

        var contentHeight = target_scroll_view.contentItem.contentHeight;
        var viewHeight = target_scroll_view.height;

        // Check if new content was added that exceeds the current view
        var contentGrew = contentHeight > root.last_content_height;
        root.last_content_height = contentHeight;

        // Only scroll if:
        // 1. Content actually grew (new content was added), AND
        // 2. The content now exceeds the view height
        if (contentGrew && contentHeight > viewHeight) {
            var maxContentY = Math.max(0, contentHeight - viewHeight);
            if (scroll_to_position(maxContentY)) {
                logger.info(`✅ ScrollableHelper: scrolled to bottom`);
            } else {
                logger.warn("ScrollableHelper: Unable to scroll - no valid scroll method found");
            }
        }
    }

    // Helper function to set scroll position using different methods
    function scroll_to_position(contentY) {
        // Try different approaches to set the scroll position
        var scrolled = false;

        // Method 1: Direct ScrollBar.vertical position
        if (target_scroll_view.ScrollBar && target_scroll_view.ScrollBar.vertical) {
            var maxContentY = Math.max(0, target_scroll_view.contentItem.contentHeight - target_scroll_view.height);
            if (maxContentY > 0) {
                var position = contentY / maxContentY;
                target_scroll_view.ScrollBar.vertical.position = Math.max(0, Math.min(1, position));
                scrolled = true;
            }
        }
        // Method 2: Try using contentY directly on the ScrollView
        else if (target_scroll_view.hasOwnProperty("contentY")) {
            target_scroll_view.contentY = contentY;
            scrolled = true;
        }
        // Method 3: Try using contentY on contentItem
        else if (target_scroll_view.contentItem.hasOwnProperty("contentY")) {
            target_scroll_view.contentItem.contentY = contentY;
            scrolled = true;
        }

        return scrolled;
    }

    // Function to initialize the helper (should be called when the scroll view is ready)
    function initialize() {
        if (target_scroll_view && target_scroll_view.contentItem) {
            root.last_content_height = target_scroll_view.contentItem.contentHeight;
        } else {
            // Reset to 0 if no valid target
            root.last_content_height = 0;
        }
    }
}
