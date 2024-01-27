# Form implementation generated from reading ui file 'simsapa/assets/ui/sutta_study_window.ui'
#
# Created by: PyQt6 UI code generator 6.5.3
#
# WARNING: Any manual changes made to this file will be lost when pyuic6 is
# run again.  Do not edit this file unless you know what you are doing.


from PyQt6 import QtCore, QtGui, QtWidgets


class Ui_SuttaStudyWindow(object):
    def setupUi(self, SuttaStudyWindow):
        SuttaStudyWindow.setObjectName("SuttaStudyWindow")
        SuttaStudyWindow.resize(1068, 625)
        self.central_widget = QtWidgets.QWidget(parent=SuttaStudyWindow)
        self.central_widget.setObjectName("central_widget")
        self.horizontalLayout_2 = QtWidgets.QHBoxLayout(self.central_widget)
        self.horizontalLayout_2.setObjectName("horizontalLayout_2")
        self.main_layout = QtWidgets.QVBoxLayout()
        self.main_layout.setObjectName("main_layout")
        self.horizontalLayout_2.addLayout(self.main_layout)
        SuttaStudyWindow.setCentralWidget(self.central_widget)
        self.menubar = QtWidgets.QMenuBar(parent=SuttaStudyWindow)
        self.menubar.setGeometry(QtCore.QRect(0, 0, 1068, 25))
        self.menubar.setObjectName("menubar")
        self.menu_File = QtWidgets.QMenu(parent=self.menubar)
        self.menu_File.setObjectName("menu_File")
        self.menu_Edit = QtWidgets.QMenu(parent=self.menubar)
        self.menu_Edit.setObjectName("menu_Edit")
        self.menu_Windows = QtWidgets.QMenu(parent=self.menubar)
        self.menu_Windows.setObjectName("menu_Windows")
        self.menu_Help = QtWidgets.QMenu(parent=self.menubar)
        self.menu_Help.setObjectName("menu_Help")
        self.menu_Suttas = QtWidgets.QMenu(parent=self.menubar)
        self.menu_Suttas.setObjectName("menu_Suttas")
        self.menu_Find = QtWidgets.QMenu(parent=self.menubar)
        self.menu_Find.setObjectName("menu_Find")
        self.menu_View = QtWidgets.QMenu(parent=self.menubar)
        self.menu_View.setObjectName("menu_View")
        SuttaStudyWindow.setMenuBar(self.menubar)
        self.toolBar = QtWidgets.QToolBar(parent=SuttaStudyWindow)
        self.toolBar.setObjectName("toolBar")
        SuttaStudyWindow.addToolBar(QtCore.Qt.ToolBarArea.LeftToolBarArea, self.toolBar)
        self.action_Copy = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Copy.setObjectName("action_Copy")
        self.action_Paste = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Paste.setObjectName("action_Paste")
        self.action_Quit = QtGui.QAction(parent=SuttaStudyWindow)
        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/close"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.action_Quit.setIcon(icon)
        self.action_Quit.setObjectName("action_Quit")
        self.action_Sutta_Search = QtGui.QAction(parent=SuttaStudyWindow)
        icon1 = QtGui.QIcon()
        icon1.addPixmap(QtGui.QPixmap(":/book"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.action_Sutta_Search.setIcon(icon1)
        self.action_Sutta_Search.setObjectName("action_Sutta_Search")
        self.action_Dictionary_Search = QtGui.QAction(parent=SuttaStudyWindow)
        icon2 = QtGui.QIcon()
        icon2.addPixmap(QtGui.QPixmap(":/dictionary"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.action_Dictionary_Search.setIcon(icon2)
        self.action_Dictionary_Search.setObjectName("action_Dictionary_Search")
        self.action_About = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_About.setObjectName("action_About")
        self.action_Website = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Website.setObjectName("action_Website")
        self.action_Close_Window = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Close_Window.setObjectName("action_Close_Window")
        self.action_Open = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Open.setObjectName("action_Open")
        self.action_Document_Reader = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Document_Reader.setObjectName("action_Document_Reader")
        self.action_Library = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Library.setObjectName("action_Library")
        self.action_Memos = QtGui.QAction(parent=SuttaStudyWindow)
        icon3 = QtGui.QIcon()
        icon3.addPixmap(QtGui.QPixmap(":/pen-fancy"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.action_Memos.setIcon(icon3)
        self.action_Memos.setObjectName("action_Memos")
        self.action_Dictionaries_Manager = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Dictionaries_Manager.setObjectName("action_Dictionaries_Manager")
        self.action_Links = QtGui.QAction(parent=SuttaStudyWindow)
        icon4 = QtGui.QIcon()
        icon4.addPixmap(QtGui.QPixmap(":/diagram"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.action_Links.setIcon(icon4)
        self.action_Links.setObjectName("action_Links")
        self.action_Search_Query_Terms = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Search_Query_Terms.setObjectName("action_Search_Query_Terms")
        self.action_Lookup_Clipboard_in_Suttas = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Lookup_Clipboard_in_Suttas.setShortcutContext(QtCore.Qt.ShortcutContext.ApplicationShortcut)
        self.action_Lookup_Clipboard_in_Suttas.setObjectName("action_Lookup_Clipboard_in_Suttas")
        self.action_Re_index_database = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Re_index_database.setObjectName("action_Re_index_database")
        self.action_Re_download_database = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Re_download_database.setObjectName("action_Re_download_database")
        self.action_Notify_About_Simsapa_Updates = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Notify_About_Simsapa_Updates.setCheckable(True)
        self.action_Notify_About_Simsapa_Updates.setChecked(True)
        self.action_Notify_About_Simsapa_Updates.setObjectName("action_Notify_About_Simsapa_Updates")
        self.action_Notify_About_DPD_Updates = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Notify_About_DPD_Updates.setCheckable(True)
        self.action_Notify_About_DPD_Updates.setChecked(True)
        self.action_Notify_About_DPD_Updates.setObjectName("action_Notify_About_DPD_Updates")
        self.action_Lookup_Clipboard_in_Dictionary = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Lookup_Clipboard_in_Dictionary.setShortcutContext(QtCore.Qt.ShortcutContext.ApplicationShortcut)
        self.action_Lookup_Clipboard_in_Dictionary.setObjectName("action_Lookup_Clipboard_in_Dictionary")
        self.action_Show_Toolbar = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Toolbar.setCheckable(True)
        self.action_Show_Toolbar.setObjectName("action_Show_Toolbar")
        self.action_First_Window_on_Startup = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_First_Window_on_Startup.setObjectName("action_First_Window_on_Startup")
        self.action_Focus_Search_Input = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Focus_Search_Input.setObjectName("action_Focus_Search_Input")
        self.action_Find_in_Page = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Find_in_Page.setObjectName("action_Find_in_Page")
        self.action_Show_Word_Lookup = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Word_Lookup.setCheckable(True)
        self.action_Show_Word_Lookup.setObjectName("action_Show_Word_Lookup")
        self.action_Show_Related_Suttas = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Related_Suttas.setCheckable(True)
        self.action_Show_Related_Suttas.setObjectName("action_Show_Related_Suttas")
        self.action_Next_Result = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Next_Result.setObjectName("action_Next_Result")
        self.action_Previous_Result = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Previous_Result.setObjectName("action_Previous_Result")
        self.action_Previous_Tab = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Previous_Tab.setObjectName("action_Previous_Tab")
        self.action_Next_Tab = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Next_Tab.setObjectName("action_Next_Tab")
        self.action_Sutta_Study = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Sutta_Study.setIcon(icon1)
        self.action_Sutta_Study.setObjectName("action_Sutta_Study")
        self.action_Lookup_Selection_in_Dictionary = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Lookup_Selection_in_Dictionary.setObjectName("action_Lookup_Selection_in_Dictionary")
        self.action_Increase_Text_Size = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Increase_Text_Size.setObjectName("action_Increase_Text_Size")
        self.action_Decrease_Text_Size = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Decrease_Text_Size.setObjectName("action_Decrease_Text_Size")
        self.action_Increase_Text_Margins = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Increase_Text_Margins.setObjectName("action_Increase_Text_Margins")
        self.action_Decrease_Text_Margins = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Decrease_Text_Margins.setObjectName("action_Decrease_Text_Margins")
        self.action_Search_Result_Sizes = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Search_Result_Sizes.setObjectName("action_Search_Result_Sizes")
        self.action_Bookmarks = QtGui.QAction(parent=SuttaStudyWindow)
        icon5 = QtGui.QIcon()
        icon5.addPixmap(QtGui.QPixmap(":/bookmark"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.action_Bookmarks.setIcon(icon5)
        self.action_Bookmarks.setObjectName("action_Bookmarks")
        self.action_Pali_Courses = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Pali_Courses.setIcon(icon1)
        self.action_Pali_Courses.setObjectName("action_Pali_Courses")
        self.action_Show_Search_Bar = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Search_Bar.setCheckable(True)
        self.action_Show_Search_Bar.setChecked(True)
        self.action_Show_Search_Bar.setStatusTip("")
        self.action_Show_Search_Bar.setObjectName("action_Show_Search_Bar")
        self.action_Show_Search_Options = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Search_Options.setCheckable(True)
        self.action_Show_Search_Options.setChecked(True)
        self.action_Show_Search_Options.setObjectName("action_Show_Search_Options")
        self.action_Search_As_You_Type = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Search_As_You_Type.setCheckable(True)
        self.action_Search_As_You_Type.setChecked(True)
        self.action_Search_As_You_Type.setStatusTip("")
        self.action_Search_As_You_Type.setObjectName("action_Search_As_You_Type")
        self.action_Search_Completion = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Search_Completion.setCheckable(True)
        self.action_Search_Completion.setChecked(True)
        self.action_Search_Completion.setObjectName("action_Search_Completion")
        self.action_Show_All_Variant_Readings = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_All_Variant_Readings.setCheckable(True)
        self.action_Show_All_Variant_Readings.setChecked(False)
        self.action_Show_All_Variant_Readings.setObjectName("action_Show_All_Variant_Readings")
        self.action_Show_Translation_and_Pali_Line_by_Line = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Translation_and_Pali_Line_by_Line.setCheckable(True)
        self.action_Show_Translation_and_Pali_Line_by_Line.setObjectName("action_Show_Translation_and_Pali_Line_by_Line")
        self.action_Reload_Page = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Reload_Page.setObjectName("action_Reload_Page")
        self.action_Show_Bookmarks = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Show_Bookmarks.setCheckable(True)
        self.action_Show_Bookmarks.setChecked(True)
        self.action_Show_Bookmarks.setObjectName("action_Show_Bookmarks")
        self.action_Study_Dictionary_Placement = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Study_Dictionary_Placement.setObjectName("action_Study_Dictionary_Placement")
        self.action_Link_Preview = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Link_Preview.setCheckable(True)
        self.action_Link_Preview.setChecked(True)
        self.action_Link_Preview.setObjectName("action_Link_Preview")
        self.action_Ebook_Reader = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Ebook_Reader.setIcon(icon1)
        self.action_Ebook_Reader.setObjectName("action_Ebook_Reader")
        self.action_Check_for_Simsapa_Updates = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Check_for_Simsapa_Updates.setObjectName("action_Check_for_Simsapa_Updates")
        self.action_Check_for_DPD_Updates = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Check_for_DPD_Updates.setObjectName("action_Check_for_DPD_Updates")
        self.action_Double_Click_on_a_Word_for_Dictionary_Lookup = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setCheckable(True)
        self.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setChecked(True)
        self.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setObjectName("action_Double_Click_on_a_Word_for_Dictionary_Lookup")
        self.action_Clipboard_Monitoring_for_Dictionary_Lookup = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Clipboard_Monitoring_for_Dictionary_Lookup.setCheckable(True)
        self.action_Clipboard_Monitoring_for_Dictionary_Lookup.setObjectName("action_Clipboard_Monitoring_for_Dictionary_Lookup")
        self.action_Sutta_Index = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Sutta_Index.setIcon(icon1)
        self.action_Sutta_Index.setObjectName("action_Sutta_Index")
        self.action_Keep_Running_in_the_Background = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Keep_Running_in_the_Background.setCheckable(True)
        self.action_Keep_Running_in_the_Background.setChecked(True)
        self.action_Keep_Running_in_the_Background.setObjectName("action_Keep_Running_in_the_Background")
        self.action_Tray_Click_Opens_Window = QtGui.QAction(parent=SuttaStudyWindow)
        self.action_Tray_Click_Opens_Window.setObjectName("action_Tray_Click_Opens_Window")
        self.menu_File.addAction(self.action_Open)
        self.menu_File.addAction(self.action_Close_Window)
        self.menu_File.addAction(self.action_Keep_Running_in_the_Background)
        self.menu_File.addAction(self.action_Tray_Click_Opens_Window)
        self.menu_File.addAction(self.action_Re_index_database)
        self.menu_File.addAction(self.action_Re_download_database)
        self.menu_File.addSeparator()
        self.menu_File.addAction(self.action_Quit)
        self.menu_Edit.addAction(self.action_Copy)
        self.menu_Edit.addAction(self.action_Paste)
        self.menu_Edit.addAction(self.action_Find_in_Page)
        self.menu_Edit.addAction(self.action_Focus_Search_Input)
        self.menu_Edit.addAction(self.action_Lookup_Selection_in_Dictionary)
        self.menu_Edit.addAction(self.action_Lookup_Clipboard_in_Suttas)
        self.menu_Edit.addAction(self.action_Lookup_Clipboard_in_Dictionary)
        self.menu_Windows.addAction(self.action_Sutta_Search)
        self.menu_Windows.addAction(self.action_Sutta_Study)
        self.menu_Windows.addAction(self.action_Sutta_Index)
        self.menu_Windows.addAction(self.action_Dictionary_Search)
        self.menu_Windows.addAction(self.action_Show_Word_Lookup)
        self.menu_Windows.addAction(self.action_Bookmarks)
        self.menu_Windows.addAction(self.action_Pali_Courses)
        self.menu_Windows.addAction(self.action_Dictionaries_Manager)
        self.menu_Windows.addAction(self.action_Document_Reader)
        self.menu_Windows.addAction(self.action_Library)
        self.menu_Windows.addAction(self.action_Ebook_Reader)
        self.menu_Windows.addAction(self.action_Memos)
        self.menu_Windows.addAction(self.action_Links)
        self.menu_Windows.addAction(self.action_First_Window_on_Startup)
        self.menu_Windows.addAction(self.action_Show_Toolbar)
        self.menu_Windows.addAction(self.action_Link_Preview)
        self.menu_Help.addAction(self.action_Search_Query_Terms)
        self.menu_Help.addAction(self.action_Check_for_Simsapa_Updates)
        self.menu_Help.addAction(self.action_Check_for_DPD_Updates)
        self.menu_Help.addAction(self.action_Notify_About_Simsapa_Updates)
        self.menu_Help.addAction(self.action_Notify_About_DPD_Updates)
        self.menu_Help.addAction(self.action_Website)
        self.menu_Help.addAction(self.action_About)
        self.menu_Suttas.addAction(self.action_Show_Related_Suttas)
        self.menu_Suttas.addAction(self.action_Show_Translation_and_Pali_Line_by_Line)
        self.menu_Suttas.addAction(self.action_Show_All_Variant_Readings)
        self.menu_Suttas.addAction(self.action_Show_Bookmarks)
        self.menu_Suttas.addAction(self.action_Study_Dictionary_Placement)
        self.menu_Find.addAction(self.action_Show_Search_Bar)
        self.menu_Find.addAction(self.action_Show_Search_Options)
        self.menu_Find.addAction(self.action_Search_As_You_Type)
        self.menu_Find.addAction(self.action_Search_Completion)
        self.menu_Find.addAction(self.action_Double_Click_on_a_Word_for_Dictionary_Lookup)
        self.menu_Find.addAction(self.action_Clipboard_Monitoring_for_Dictionary_Lookup)
        self.menu_Find.addAction(self.action_Previous_Result)
        self.menu_Find.addAction(self.action_Next_Result)
        self.menu_Find.addAction(self.action_Previous_Tab)
        self.menu_Find.addAction(self.action_Next_Tab)
        self.menu_View.addAction(self.action_Reload_Page)
        self.menu_View.addAction(self.action_Increase_Text_Size)
        self.menu_View.addAction(self.action_Decrease_Text_Size)
        self.menu_View.addAction(self.action_Increase_Text_Margins)
        self.menu_View.addAction(self.action_Decrease_Text_Margins)
        self.menu_View.addAction(self.action_Search_Result_Sizes)
        self.menubar.addAction(self.menu_File.menuAction())
        self.menubar.addAction(self.menu_Edit.menuAction())
        self.menubar.addAction(self.menu_View.menuAction())
        self.menubar.addAction(self.menu_Find.menuAction())
        self.menubar.addAction(self.menu_Windows.menuAction())
        self.menubar.addAction(self.menu_Suttas.menuAction())
        self.menubar.addAction(self.menu_Help.menuAction())
        self.toolBar.addAction(self.action_Sutta_Search)
        self.toolBar.addAction(self.action_Dictionary_Search)
        self.toolBar.addAction(self.action_Memos)
        self.toolBar.addAction(self.action_Links)

        self.retranslateUi(SuttaStudyWindow)
        QtCore.QMetaObject.connectSlotsByName(SuttaStudyWindow)

    def retranslateUi(self, SuttaStudyWindow):
        _translate = QtCore.QCoreApplication.translate
        SuttaStudyWindow.setWindowTitle(_translate("SuttaStudyWindow", "Sutta Study - Simsapa"))
        self.menu_File.setTitle(_translate("SuttaStudyWindow", "&File"))
        self.menu_Edit.setTitle(_translate("SuttaStudyWindow", "&Edit"))
        self.menu_Windows.setTitle(_translate("SuttaStudyWindow", "&Windows"))
        self.menu_Help.setTitle(_translate("SuttaStudyWindow", "&Help"))
        self.menu_Suttas.setTitle(_translate("SuttaStudyWindow", "&Suttas"))
        self.menu_Find.setTitle(_translate("SuttaStudyWindow", "F&ind"))
        self.menu_View.setTitle(_translate("SuttaStudyWindow", "&View"))
        self.toolBar.setWindowTitle(_translate("SuttaStudyWindow", "toolBar"))
        self.action_Copy.setText(_translate("SuttaStudyWindow", "&Copy"))
        self.action_Copy.setShortcut(_translate("SuttaStudyWindow", "Ctrl+C"))
        self.action_Paste.setText(_translate("SuttaStudyWindow", "&Paste"))
        self.action_Paste.setShortcut(_translate("SuttaStudyWindow", "Ctrl+V"))
        self.action_Quit.setText(_translate("SuttaStudyWindow", "&Quit Simsapa"))
        self.action_Quit.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Q"))
        self.action_Sutta_Search.setText(_translate("SuttaStudyWindow", "&Sutta Search"))
        self.action_Sutta_Search.setShortcut(_translate("SuttaStudyWindow", "F5"))
        self.action_Dictionary_Search.setText(_translate("SuttaStudyWindow", "&Dictionary Search"))
        self.action_Dictionary_Search.setShortcut(_translate("SuttaStudyWindow", "F6"))
        self.action_About.setText(_translate("SuttaStudyWindow", "&About"))
        self.action_Website.setText(_translate("SuttaStudyWindow", "&Website"))
        self.action_Close_Window.setText(_translate("SuttaStudyWindow", "&Close Window"))
        self.action_Open.setText(_translate("SuttaStudyWindow", "&Open..."))
        self.action_Open.setShortcut(_translate("SuttaStudyWindow", "Ctrl+O"))
        self.action_Document_Reader.setText(_translate("SuttaStudyWindow", "D&ocument Reader"))
        self.action_Document_Reader.setToolTip(_translate("SuttaStudyWindow", "Document Reader"))
        self.action_Document_Reader.setShortcut(_translate("SuttaStudyWindow", "F7"))
        self.action_Library.setText(_translate("SuttaStudyWindow", "&Library"))
        self.action_Library.setShortcut(_translate("SuttaStudyWindow", "F8"))
        self.action_Memos.setText(_translate("SuttaStudyWindow", "&Memos"))
        self.action_Memos.setToolTip(_translate("SuttaStudyWindow", "Memos"))
        self.action_Memos.setShortcut(_translate("SuttaStudyWindow", "F9"))
        self.action_Dictionaries_Manager.setText(_translate("SuttaStudyWindow", "Dictionaries &Manager"))
        self.action_Dictionaries_Manager.setShortcut(_translate("SuttaStudyWindow", "F10"))
        self.action_Links.setText(_translate("SuttaStudyWindow", "&Links"))
        self.action_Links.setShortcut(_translate("SuttaStudyWindow", "F11"))
        self.action_Search_Query_Terms.setText(_translate("SuttaStudyWindow", "Search Query Terms"))
        self.action_Lookup_Clipboard_in_Suttas.setText(_translate("SuttaStudyWindow", "&Lookup Clipboard in Suttas"))
        self.action_Lookup_Clipboard_in_Suttas.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Shift+S"))
        self.action_Re_index_database.setText(_translate("SuttaStudyWindow", "Re-index database..."))
        self.action_Re_download_database.setText(_translate("SuttaStudyWindow", "Re-download database..."))
        self.action_Notify_About_Simsapa_Updates.setText(_translate("SuttaStudyWindow", "Notify About Simsapa Updates"))
        self.action_Notify_About_DPD_Updates.setText(_translate("SuttaStudyWindow", "Notify About DPD Updates"))
        self.action_Lookup_Clipboard_in_Dictionary.setText(_translate("SuttaStudyWindow", "&Lookup Clipboard in Dictionary"))
        self.action_Lookup_Clipboard_in_Dictionary.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Shift+G"))
        self.action_Show_Toolbar.setText(_translate("SuttaStudyWindow", "Show Toolbar"))
        self.action_First_Window_on_Startup.setText(_translate("SuttaStudyWindow", "First Window on Startup..."))
        self.action_Focus_Search_Input.setText(_translate("SuttaStudyWindow", "Focus Search Input"))
        self.action_Focus_Search_Input.setShortcut(_translate("SuttaStudyWindow", "Ctrl+L"))
        self.action_Find_in_Page.setText(_translate("SuttaStudyWindow", "Find in Page..."))
        self.action_Find_in_Page.setShortcut(_translate("SuttaStudyWindow", "Ctrl+F"))
        self.action_Show_Word_Lookup.setText(_translate("SuttaStudyWindow", "Show Word Lookup"))
        self.action_Show_Word_Lookup.setShortcut(_translate("SuttaStudyWindow", "Ctrl+F6"))
        self.action_Show_Related_Suttas.setText(_translate("SuttaStudyWindow", "Show Related Suttas"))
        self.action_Next_Result.setText(_translate("SuttaStudyWindow", "Next Result"))
        self.action_Next_Result.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Down"))
        self.action_Previous_Result.setText(_translate("SuttaStudyWindow", "Previous Result"))
        self.action_Previous_Result.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Up"))
        self.action_Previous_Tab.setText(_translate("SuttaStudyWindow", "Previous Tab"))
        self.action_Previous_Tab.setShortcut(_translate("SuttaStudyWindow", "Ctrl+PgUp"))
        self.action_Next_Tab.setText(_translate("SuttaStudyWindow", "Next Tab"))
        self.action_Next_Tab.setShortcut(_translate("SuttaStudyWindow", "Ctrl+PgDown"))
        self.action_Sutta_Study.setText(_translate("SuttaStudyWindow", "Sutta Study"))
        self.action_Sutta_Study.setShortcut(_translate("SuttaStudyWindow", "Ctrl+F5"))
        self.action_Lookup_Selection_in_Dictionary.setText(_translate("SuttaStudyWindow", "Lookup Selection in Dictionary"))
        self.action_Lookup_Selection_in_Dictionary.setShortcut(_translate("SuttaStudyWindow", "Ctrl+G"))
        self.action_Increase_Text_Size.setText(_translate("SuttaStudyWindow", "&Increase Text Size"))
        self.action_Increase_Text_Size.setShortcut(_translate("SuttaStudyWindow", "Ctrl++"))
        self.action_Decrease_Text_Size.setText(_translate("SuttaStudyWindow", "&Decrease Text Size"))
        self.action_Decrease_Text_Size.setShortcut(_translate("SuttaStudyWindow", "Ctrl+-"))
        self.action_Increase_Text_Margins.setText(_translate("SuttaStudyWindow", "Increase Text Margins"))
        self.action_Increase_Text_Margins.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Shift++"))
        self.action_Decrease_Text_Margins.setText(_translate("SuttaStudyWindow", "Decrease Text Margins"))
        self.action_Decrease_Text_Margins.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Shift+-"))
        self.action_Search_Result_Sizes.setText(_translate("SuttaStudyWindow", "&Search Result Sizes..."))
        self.action_Bookmarks.setText(_translate("SuttaStudyWindow", "&Bookmarks"))
        self.action_Bookmarks.setShortcut(_translate("SuttaStudyWindow", "F7"))
        self.action_Pali_Courses.setText(_translate("SuttaStudyWindow", "&Pali Courses"))
        self.action_Pali_Courses.setShortcut(_translate("SuttaStudyWindow", "F8"))
        self.action_Show_Search_Bar.setText(_translate("SuttaStudyWindow", "Show Search &Bar"))
        self.action_Show_Search_Bar.setShortcut(_translate("SuttaStudyWindow", "Ctrl+Shift+/"))
        self.action_Show_Search_Bar.setToolTip(_translate("SuttaStudyWindow", "Show or hide the search bar with input field and controls"))
        self.action_Show_Search_Options.setText(_translate("SuttaStudyWindow", "Show Search Options"))
        self.action_Show_Search_Options.setToolTip(_translate("SuttaStudyWindow", "Show or the search control options"))
        self.action_Search_As_You_Type.setText(_translate("SuttaStudyWindow", "Search As You &Type"))
        self.action_Search_As_You_Type.setToolTip(_translate("SuttaStudyWindow", "Run search query when you stop typing"))
        self.action_Search_Completion.setText(_translate("SuttaStudyWindow", "Search &Completion"))
        self.action_Show_All_Variant_Readings.setText(_translate("SuttaStudyWindow", "Show All &Variant Readings"))
        self.action_Show_Translation_and_Pali_Line_by_Line.setText(_translate("SuttaStudyWindow", "Show Translation and Pali &Line by Line"))
        self.action_Reload_Page.setText(_translate("SuttaStudyWindow", "&Reload Page"))
        self.action_Reload_Page.setShortcut(_translate("SuttaStudyWindow", "Ctrl+R"))
        self.action_Show_Bookmarks.setText(_translate("SuttaStudyWindow", "Show &Bookmarks"))
        self.action_Study_Dictionary_Placement.setText(_translate("SuttaStudyWindow", "Study Dictionary Placement..."))
        self.action_Link_Preview.setText(_translate("SuttaStudyWindow", "Link Preview"))
        self.action_Ebook_Reader.setText(_translate("SuttaStudyWindow", "&Ebook Reader"))
        self.action_Check_for_Simsapa_Updates.setText(_translate("SuttaStudyWindow", "Check for Simsapa Updates..."))
        self.action_Check_for_DPD_Updates.setText(_translate("SuttaStudyWindow", "Check for DPD Updates..."))
        self.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setText(_translate("SuttaStudyWindow", "Double Click on a Word for Dictionary Lookup"))
        self.action_Clipboard_Monitoring_for_Dictionary_Lookup.setText(_translate("SuttaStudyWindow", "Clipboard Monitoring for Dictionary Lookup"))
        self.action_Sutta_Index.setText(_translate("SuttaStudyWindow", "Sutta Index"))
        self.action_Keep_Running_in_the_Background.setText(_translate("SuttaStudyWindow", "Keep Running in the Background"))
        self.action_Tray_Click_Opens_Window.setText(_translate("SuttaStudyWindow", "Tray Click Opens Window..."))
