# -*- coding: utf-8 -*-

# Form implementation generated from reading ui file 'simsapa/assets/ui/links_browser_window.ui'
#
# Created by: PyQt5 UI code generator 5.15.6
#
# WARNING: Any manual changes made to this file will be lost when pyuic5 is
# run again.  Do not edit this file unless you know what you are doing.


from PyQt5 import QtCore, QtGui, QtWidgets


class Ui_LinksBrowserWindow(object):
    def setupUi(self, LinksBrowserWindow):
        LinksBrowserWindow.setObjectName("LinksBrowserWindow")
        LinksBrowserWindow.resize(1068, 643)
        LinksBrowserWindow.setBaseSize(QtCore.QSize(800, 600))
        self.central_widget = QtWidgets.QWidget(LinksBrowserWindow)
        self.central_widget.setObjectName("central_widget")
        self.horizontalLayout_2 = QtWidgets.QHBoxLayout(self.central_widget)
        self.horizontalLayout_2.setObjectName("horizontalLayout_2")
        self.main_layout = QtWidgets.QVBoxLayout()
        self.main_layout.setObjectName("main_layout")
        self.splitter = QtWidgets.QSplitter(self.central_widget)
        self.splitter.setOrientation(QtCore.Qt.Horizontal)
        self.splitter.setObjectName("splitter")
        self.verticalLayoutWidget_2 = QtWidgets.QWidget(self.splitter)
        self.verticalLayoutWidget_2.setObjectName("verticalLayoutWidget_2")
        self.links_layout = QtWidgets.QVBoxLayout(self.verticalLayoutWidget_2)
        self.links_layout.setContentsMargins(0, 0, 0, 0)
        self.links_layout.setObjectName("links_layout")
        spacerItem = QtWidgets.QSpacerItem(500, 20, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        self.links_layout.addItem(spacerItem)
        self.verticalLayoutWidget_3 = QtWidgets.QWidget(self.splitter)
        self.verticalLayoutWidget_3.setObjectName("verticalLayoutWidget_3")
        self.tabs_layout = QtWidgets.QVBoxLayout(self.verticalLayoutWidget_3)
        self.tabs_layout.setContentsMargins(0, 0, 0, 0)
        self.tabs_layout.setObjectName("tabs_layout")
        self.horizontalLayout = QtWidgets.QHBoxLayout()
        self.horizontalLayout.setObjectName("horizontalLayout")
        self.create_link_btn = QtWidgets.QPushButton(self.verticalLayoutWidget_3)
        self.create_link_btn.setObjectName("create_link_btn")
        self.horizontalLayout.addWidget(self.create_link_btn)
        self.clear_link_btn = QtWidgets.QPushButton(self.verticalLayoutWidget_3)
        self.clear_link_btn.setObjectName("clear_link_btn")
        self.horizontalLayout.addWidget(self.clear_link_btn)
        self.remove_link_btn = QtWidgets.QPushButton(self.verticalLayoutWidget_3)
        self.remove_link_btn.setObjectName("remove_link_btn")
        self.horizontalLayout.addWidget(self.remove_link_btn)
        self.tabs_layout.addLayout(self.horizontalLayout)
        self.horizontalLayout_5 = QtWidgets.QHBoxLayout()
        self.horizontalLayout_5.setObjectName("horizontalLayout_5")
        self.verticalLayout_3 = QtWidgets.QVBoxLayout()
        self.verticalLayout_3.setObjectName("verticalLayout_3")
        self.set_from_btn = QtWidgets.QPushButton(self.verticalLayoutWidget_3)
        self.set_from_btn.setObjectName("set_from_btn")
        self.verticalLayout_3.addWidget(self.set_from_btn)
        self.from_view = QtWidgets.QPlainTextEdit(self.verticalLayoutWidget_3)
        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.from_view.sizePolicy().hasHeightForWidth())
        self.from_view.setSizePolicy(sizePolicy)
        self.from_view.setMaximumSize(QtCore.QSize(16777215, 80))
        self.from_view.setFrameShape(QtWidgets.QFrame.NoFrame)
        self.from_view.setObjectName("from_view")
        self.verticalLayout_3.addWidget(self.from_view)
        self.horizontalLayout_5.addLayout(self.verticalLayout_3)
        self.verticalLayout_2 = QtWidgets.QVBoxLayout()
        self.verticalLayout_2.setObjectName("verticalLayout_2")
        self.set_to_btn = QtWidgets.QPushButton(self.verticalLayoutWidget_3)
        self.set_to_btn.setObjectName("set_to_btn")
        self.verticalLayout_2.addWidget(self.set_to_btn)
        self.to_view = QtWidgets.QPlainTextEdit(self.verticalLayoutWidget_3)
        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.to_view.sizePolicy().hasHeightForWidth())
        self.to_view.setSizePolicy(sizePolicy)
        self.to_view.setMaximumSize(QtCore.QSize(16777215, 80))
        self.to_view.setFrameShape(QtWidgets.QFrame.NoFrame)
        self.to_view.setObjectName("to_view")
        self.verticalLayout_2.addWidget(self.to_view)
        self.horizontalLayout_5.addLayout(self.verticalLayout_2)
        self.tabs_layout.addLayout(self.horizontalLayout_5)
        self.searchbar_layout = QtWidgets.QVBoxLayout()
        self.searchbar_layout.setObjectName("searchbar_layout")
        self.horizontalLayout_4 = QtWidgets.QHBoxLayout()
        self.horizontalLayout_4.setObjectName("horizontalLayout_4")
        self.search_input = QtWidgets.QLineEdit(self.verticalLayoutWidget_3)
        self.search_input.setMinimumSize(QtCore.QSize(0, 35))
        self.search_input.setObjectName("search_input")
        self.horizontalLayout_4.addWidget(self.search_input)
        self.search_button = QtWidgets.QPushButton(self.verticalLayoutWidget_3)
        self.search_button.setObjectName("search_button")
        self.horizontalLayout_4.addWidget(self.search_button)
        self.link_table = QtWidgets.QComboBox(self.verticalLayoutWidget_3)
        self.link_table.setObjectName("link_table")
        self.link_table.addItem("")
        self.link_table.addItem("")
        self.link_table.addItem("")
        self.horizontalLayout_4.addWidget(self.link_table)
        self.searchbar_layout.addLayout(self.horizontalLayout_4)
        self.horizontalLayout_6 = QtWidgets.QHBoxLayout()
        self.horizontalLayout_6.setObjectName("horizontalLayout_6")
        spacerItem1 = QtWidgets.QSpacerItem(40, 20, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        self.horizontalLayout_6.addItem(spacerItem1)
        self.pali_buttons_layout = QtWidgets.QVBoxLayout()
        self.pali_buttons_layout.setObjectName("pali_buttons_layout")
        self.horizontalLayout_6.addLayout(self.pali_buttons_layout)
        spacerItem2 = QtWidgets.QSpacerItem(40, 20, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        self.horizontalLayout_6.addItem(spacerItem2)
        self.searchbar_layout.addLayout(self.horizontalLayout_6)
        self.tabs_layout.addLayout(self.searchbar_layout)
        self.horizontalLayout_3 = QtWidgets.QHBoxLayout()
        self.horizontalLayout_3.setObjectName("horizontalLayout_3")
        self.set_page_number = QtWidgets.QCheckBox(self.verticalLayoutWidget_3)
        self.set_page_number.setEnabled(False)
        self.set_page_number.setObjectName("set_page_number")
        self.horizontalLayout_3.addWidget(self.set_page_number)
        self.page_number = QtWidgets.QSpinBox(self.verticalLayoutWidget_3)
        self.page_number.setEnabled(False)
        self.page_number.setObjectName("page_number")
        self.horizontalLayout_3.addWidget(self.page_number)
        self.tabs_layout.addLayout(self.horizontalLayout_3)
        self.results_list = QtWidgets.QListWidget(self.verticalLayoutWidget_3)
        self.results_list.setFrameShape(QtWidgets.QFrame.NoFrame)
        self.results_list.setLineWidth(1)
        self.results_list.setObjectName("results_list")
        self.tabs_layout.addWidget(self.results_list)
        self.main_layout.addWidget(self.splitter)
        self.horizontalLayout_2.addLayout(self.main_layout)
        LinksBrowserWindow.setCentralWidget(self.central_widget)
        self.menubar = QtWidgets.QMenuBar(LinksBrowserWindow)
        self.menubar.setGeometry(QtCore.QRect(0, 0, 1068, 25))
        self.menubar.setObjectName("menubar")
        self.menu_File = QtWidgets.QMenu(self.menubar)
        self.menu_File.setObjectName("menu_File")
        self.menu_Edit = QtWidgets.QMenu(self.menubar)
        self.menu_Edit.setObjectName("menu_Edit")
        self.menu_Windows = QtWidgets.QMenu(self.menubar)
        self.menu_Windows.setObjectName("menu_Windows")
        self.menu_Help = QtWidgets.QMenu(self.menubar)
        self.menu_Help.setObjectName("menu_Help")
        LinksBrowserWindow.setMenuBar(self.menubar)
        self.statusbar = QtWidgets.QStatusBar(LinksBrowserWindow)
        self.statusbar.setObjectName("statusbar")
        LinksBrowserWindow.setStatusBar(self.statusbar)
        self.toolBar = QtWidgets.QToolBar(LinksBrowserWindow)
        self.toolBar.setObjectName("toolBar")
        LinksBrowserWindow.addToolBar(QtCore.Qt.LeftToolBarArea, self.toolBar)
        self.action_Copy = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Copy.setObjectName("action_Copy")
        self.action_Paste = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Paste.setObjectName("action_Paste")
        self.action_Quit = QtWidgets.QAction(LinksBrowserWindow)
        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/close"), QtGui.QIcon.Normal, QtGui.QIcon.Off)
        self.action_Quit.setIcon(icon)
        self.action_Quit.setObjectName("action_Quit")
        self.action_Sutta_Search = QtWidgets.QAction(LinksBrowserWindow)
        icon1 = QtGui.QIcon()
        icon1.addPixmap(QtGui.QPixmap(":/book"), QtGui.QIcon.Normal, QtGui.QIcon.Off)
        self.action_Sutta_Search.setIcon(icon1)
        self.action_Sutta_Search.setObjectName("action_Sutta_Search")
        self.action_Dictionary_Search = QtWidgets.QAction(LinksBrowserWindow)
        icon2 = QtGui.QIcon()
        icon2.addPixmap(QtGui.QPixmap(":/dictionary"), QtGui.QIcon.Normal, QtGui.QIcon.Off)
        self.action_Dictionary_Search.setIcon(icon2)
        self.action_Dictionary_Search.setObjectName("action_Dictionary_Search")
        self.action_About = QtWidgets.QAction(LinksBrowserWindow)
        self.action_About.setObjectName("action_About")
        self.action_Website = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Website.setObjectName("action_Website")
        self.action_Close_Window = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Close_Window.setObjectName("action_Close_Window")
        self.action_Open = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Open.setObjectName("action_Open")
        self.action_Document_Reader = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Document_Reader.setObjectName("action_Document_Reader")
        self.action_Library = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Library.setObjectName("action_Library")
        self.action_Memos = QtWidgets.QAction(LinksBrowserWindow)
        icon3 = QtGui.QIcon()
        icon3.addPixmap(QtGui.QPixmap(":/pen-fancy"), QtGui.QIcon.Normal, QtGui.QIcon.Off)
        self.action_Memos.setIcon(icon3)
        self.action_Memos.setObjectName("action_Memos")
        self.action_Dictionaries_Manager = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Dictionaries_Manager.setObjectName("action_Dictionaries_Manager")
        self.action_Links = QtWidgets.QAction(LinksBrowserWindow)
        icon4 = QtGui.QIcon()
        icon4.addPixmap(QtGui.QPixmap(":/diagram"), QtGui.QIcon.Normal, QtGui.QIcon.Off)
        self.action_Links.setIcon(icon4)
        self.action_Links.setObjectName("action_Links")
        self.action_Re_index_database = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Re_index_database.setObjectName("action_Re_index_database")
        self.action_Re_download_database = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Re_download_database.setObjectName("action_Re_download_database")
        self.action_Notify_About_Updates = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Notify_About_Updates.setCheckable(True)
        self.action_Notify_About_Updates.setChecked(True)
        self.action_Notify_About_Updates.setObjectName("action_Notify_About_Updates")
        self.action_Show_Toolbar = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Show_Toolbar.setCheckable(True)
        self.action_Show_Toolbar.setObjectName("action_Show_Toolbar")
        self.action_First_Window_on_Startup = QtWidgets.QAction(LinksBrowserWindow)
        self.action_First_Window_on_Startup.setObjectName("action_First_Window_on_Startup")
        self.action_Focus_Search_Input = QtWidgets.QAction(LinksBrowserWindow)
        self.action_Focus_Search_Input.setObjectName("action_Focus_Search_Input")
        self.menu_File.addAction(self.action_Open)
        self.menu_File.addAction(self.action_Close_Window)
        self.menu_File.addAction(self.action_Re_index_database)
        self.menu_File.addAction(self.action_Re_download_database)
        self.menu_File.addSeparator()
        self.menu_File.addAction(self.action_Quit)
        self.menu_Edit.addAction(self.action_Copy)
        self.menu_Edit.addAction(self.action_Paste)
        self.menu_Edit.addAction(self.action_Focus_Search_Input)
        self.menu_Windows.addAction(self.action_Sutta_Search)
        self.menu_Windows.addAction(self.action_Dictionary_Search)
        self.menu_Windows.addAction(self.action_Dictionaries_Manager)
        self.menu_Windows.addAction(self.action_Document_Reader)
        self.menu_Windows.addAction(self.action_Library)
        self.menu_Windows.addAction(self.action_Memos)
        self.menu_Windows.addAction(self.action_Links)
        self.menu_Windows.addAction(self.action_First_Window_on_Startup)
        self.menu_Windows.addAction(self.action_Show_Toolbar)
        self.menu_Help.addAction(self.action_Notify_About_Updates)
        self.menu_Help.addAction(self.action_Website)
        self.menu_Help.addAction(self.action_About)
        self.menubar.addAction(self.menu_File.menuAction())
        self.menubar.addAction(self.menu_Edit.menuAction())
        self.menubar.addAction(self.menu_Windows.menuAction())
        self.menubar.addAction(self.menu_Help.menuAction())
        self.toolBar.addAction(self.action_Sutta_Search)
        self.toolBar.addAction(self.action_Dictionary_Search)
        self.toolBar.addAction(self.action_Memos)
        self.toolBar.addAction(self.action_Links)

        self.retranslateUi(LinksBrowserWindow)
        QtCore.QMetaObject.connectSlotsByName(LinksBrowserWindow)

    def retranslateUi(self, LinksBrowserWindow):
        _translate = QtCore.QCoreApplication.translate
        LinksBrowserWindow.setWindowTitle(_translate("LinksBrowserWindow", "Links - Simsapa"))
        self.create_link_btn.setText(_translate("LinksBrowserWindow", "Create Link"))
        self.clear_link_btn.setText(_translate("LinksBrowserWindow", "Clear"))
        self.remove_link_btn.setText(_translate("LinksBrowserWindow", "Remove Link"))
        self.set_from_btn.setText(_translate("LinksBrowserWindow", "Set From"))
        self.set_to_btn.setText(_translate("LinksBrowserWindow", "Set To"))
        self.search_button.setText(_translate("LinksBrowserWindow", "Search"))
        self.link_table.setItemText(0, _translate("LinksBrowserWindow", "Suttas"))
        self.link_table.setItemText(1, _translate("LinksBrowserWindow", "DictWords"))
        self.link_table.setItemText(2, _translate("LinksBrowserWindow", "Documents"))
        self.set_page_number.setText(_translate("LinksBrowserWindow", "Set page number:"))
        self.menu_File.setTitle(_translate("LinksBrowserWindow", "&File"))
        self.menu_Edit.setTitle(_translate("LinksBrowserWindow", "&Edit"))
        self.menu_Windows.setTitle(_translate("LinksBrowserWindow", "&Windows"))
        self.menu_Help.setTitle(_translate("LinksBrowserWindow", "&Help"))
        self.toolBar.setWindowTitle(_translate("LinksBrowserWindow", "toolBar"))
        self.action_Copy.setText(_translate("LinksBrowserWindow", "&Copy"))
        self.action_Copy.setShortcut(_translate("LinksBrowserWindow", "Ctrl+C"))
        self.action_Paste.setText(_translate("LinksBrowserWindow", "&Paste"))
        self.action_Paste.setShortcut(_translate("LinksBrowserWindow", "Ctrl+V"))
        self.action_Quit.setText(_translate("LinksBrowserWindow", "&Quit"))
        self.action_Quit.setShortcut(_translate("LinksBrowserWindow", "Ctrl+Q"))
        self.action_Sutta_Search.setText(_translate("LinksBrowserWindow", "&Sutta Search"))
        self.action_Sutta_Search.setShortcut(_translate("LinksBrowserWindow", "F5"))
        self.action_Dictionary_Search.setText(_translate("LinksBrowserWindow", "&Dictionary Search"))
        self.action_Dictionary_Search.setShortcut(_translate("LinksBrowserWindow", "F6"))
        self.action_About.setText(_translate("LinksBrowserWindow", "&About"))
        self.action_Website.setText(_translate("LinksBrowserWindow", "&Website"))
        self.action_Close_Window.setText(_translate("LinksBrowserWindow", "&Close Window"))
        self.action_Open.setText(_translate("LinksBrowserWindow", "&Open..."))
        self.action_Open.setShortcut(_translate("LinksBrowserWindow", "Ctrl+O"))
        self.action_Document_Reader.setText(_translate("LinksBrowserWindow", "D&ocument Reader"))
        self.action_Document_Reader.setToolTip(_translate("LinksBrowserWindow", "Document Reader"))
        self.action_Document_Reader.setShortcut(_translate("LinksBrowserWindow", "F7"))
        self.action_Library.setText(_translate("LinksBrowserWindow", "&Library"))
        self.action_Library.setShortcut(_translate("LinksBrowserWindow", "F8"))
        self.action_Memos.setText(_translate("LinksBrowserWindow", "&Memos"))
        self.action_Memos.setToolTip(_translate("LinksBrowserWindow", "Memos"))
        self.action_Memos.setShortcut(_translate("LinksBrowserWindow", "F9"))
        self.action_Dictionaries_Manager.setText(_translate("LinksBrowserWindow", "Dictionaries &Manager"))
        self.action_Dictionaries_Manager.setShortcut(_translate("LinksBrowserWindow", "F10"))
        self.action_Links.setText(_translate("LinksBrowserWindow", "&Links"))
        self.action_Links.setShortcut(_translate("LinksBrowserWindow", "F11"))
        self.action_Re_index_database.setText(_translate("LinksBrowserWindow", "Re-index database..."))
        self.action_Re_download_database.setText(_translate("LinksBrowserWindow", "Re-download database..."))
        self.action_Notify_About_Updates.setText(_translate("LinksBrowserWindow", "Notify About Updates"))
        self.action_Show_Toolbar.setText(_translate("LinksBrowserWindow", "Show Toolbar"))
        self.action_First_Window_on_Startup.setText(_translate("LinksBrowserWindow", "First Window on Startup..."))
        self.action_Focus_Search_Input.setText(_translate("LinksBrowserWindow", "Focus Search Input"))
        self.action_Focus_Search_Input.setShortcut(_translate("LinksBrowserWindow", "Ctrl+L"))
from simsapa.assets import icons_rc
