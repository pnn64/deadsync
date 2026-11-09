return function()
    return PREFSMAN:GetPreference('EasterEggs') and MonthOfYear() == 9 and ThemePrefs.Get('VisualStyle') == 'Spooky'
end